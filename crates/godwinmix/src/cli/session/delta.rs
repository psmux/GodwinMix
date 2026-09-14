//! State deltas: the part of a session a replay is graded on.
//!
//! A session log carries times, sequence numbers, running times and trace ids,
//! and none of them will be the same on the second run. What has to be the
//! same is what the run *did*: what went on air, in what order, which sources
//! went live and which fell over, what the ad break did, which hooks were
//! skipped. Those are the deltas.
//!
//! This is also what the operator eval grades against (09 section 5 item 20):
//! the world, not the reply. An `expect_changes.json` is a list of these.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// One thing that changed, normalised so two runs can be compared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delta {
    /// `program`, `source`, `output`, `adbreak`, `hook`, `alert`.
    pub what: String,
    /// The id it happened to: a source id, an output id, a hook name. Empty
    /// where the change is not about one thing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// The value it moved to: a source id for a take, a state name for a
    /// source or an output.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub to: String,
}

impl Delta {
    pub fn new(what: &str, id: impl Into<String>, to: impl Into<String>) -> Delta {
        Delta { what: what.into(), id: id.into(), to: to.into() }
    }

    /// One line, as the diff prints it.
    pub fn line(&self) -> String {
        match (self.id.is_empty(), self.to.is_empty()) {
            (true, true) => self.what.clone(),
            (true, false) => format!("{} -> {}", self.what, self.to),
            (false, true) => format!("{} {}", self.what, self.id),
            (false, false) => format!("{} {} -> {}", self.what, self.id, self.to),
        }
    }
}

/// An `expect_changes.json` file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Expectations {
    /// The deltas the run has to produce, in order.
    pub changes: Vec<Delta>,
}

impl Expectations {
    pub fn load(path: &Path) -> Result<Expectations> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("there is no expectations file at {}", path.display()))?;
        // Either `{"changes": [..]}` or a bare array, because the second is
        // what somebody writes by hand the first time.
        if let Ok(list) = serde_json::from_str::<Vec<Delta>>(&text) {
            return Ok(Expectations { changes: list });
        }
        serde_json::from_str(&text)
            .with_context(|| format!("{} is not an expect_changes document", path.display()))
    }
}

/// The deltas in a recorded session.
pub fn of(records: &[super::Record]) -> Vec<Delta> {
    let mut tracker = Tracker::default();
    let mut out = Vec::new();
    for record in records.iter().filter(|r| r.kind == "event") {
        if let Some(event) = record.event() {
            out.extend(tracker.absorb(event));
        }
    }
    out
}

/// Collect deltas off a live event stream for as long as it is held.
///
/// A task rather than a drain at the end: a source that fails noisily can
/// publish faster than the bus holds, and a receiver that fell behind would
/// quietly hand back a short answer. Lagging is reported and the stream is
/// followed on, because a replay with a hole in it must say so rather than
/// pass.
pub struct Collector {
    /// Written by the task as events arrive, so aborting it loses nothing.
    seen: std::sync::Arc<parking_lot::Mutex<(Vec<Delta>, u64)>>,
    task: tokio::task::JoinHandle<()>,
}

impl Collector {
    pub fn start(
        mut rx: tokio::sync::broadcast::Receiver<godwinmix_core::state::Envelope>,
    ) -> Collector {
        let seen = std::sync::Arc::new(parking_lot::Mutex::new((Vec::new(), 0u64)));
        let mine = seen.clone();
        let task = tokio::spawn(async move {
            let mut tracker = Tracker::default();
            loop {
                match rx.recv().await {
                    Ok(envelope) => {
                        let Ok(value) = serde_json::to_value(&envelope.event) else { continue };
                        let deltas = tracker.absorb(&value);
                        if !deltas.is_empty() {
                            mine.lock().0.extend(deltas);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        mine.lock().1 += n;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        Collector { seen, task }
    }

    /// Stop following and take what was collected, with how many events were
    /// missed along the way.
    pub fn finish(self) -> (Vec<Delta>, u64) {
        self.task.abort();
        let taken = std::mem::take(&mut *self.seen.lock());
        taken
    }
}

/// Turns an event stream into deltas.
///
/// Stateful because most of what changes is only visible by comparing: the
/// mixer publishes a whole `status` when the shape of the show changes, and a
/// source going from `connecting` to `live` is that document twice with one
/// field different. A tracker emits the difference, which is what a replay can
/// be graded on. Repeats are dropped for the same reason.
#[derive(Default)]
pub struct Tracker {
    sources: std::collections::BTreeMap<String, String>,
    outputs: std::collections::BTreeMap<String, String>,
    program: Option<String>,
    ad: Option<String>,
}

impl Tracker {
    /// One event, as none or more deltas.
    pub fn absorb(&mut self, event: &Value) -> Vec<Delta> {
        let s = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or("").to_string();
        let Some(kind) = event.get("type").and_then(Value::as_str) else { return Vec::new() };
        match kind {
            "took" => {
                let to = event.get("source").and_then(Value::as_str).unwrap_or("slate").to_string();
                self.program = Some(to.clone());
                vec![Delta::new("program", "", to)]
            }
            "source_state_changed" => self.source(&s(event, "source"), &s(event, "state")),
            "output_state_changed" => self.output(&s(event, "output"), &s(event, "state")),
            "ad_break_changed" => self.adbreak(event.get("ad")),
            "hook_blocked" => vec![Delta::new("hook", s(event, "hook"), "blocked")],
            "alert" => vec![Delta::new("alert", s(event, "severity"), "")],
            "status" => {
                let mut out = Vec::new();
                let empty = Vec::new();
                let sources =
                    event.get("sources").and_then(Value::as_array).unwrap_or(&empty).clone();
                let mut seen = std::collections::BTreeSet::new();
                for source in &sources {
                    let id = s(source, "id");
                    seen.insert(id.clone());
                    out.extend(self.source(&id, &s(source, "state")));
                }
                let gone: Vec<String> =
                    self.sources.keys().filter(|id| !seen.contains(*id)).cloned().collect();
                for id in gone {
                    self.sources.remove(&id);
                    out.push(Delta::new("source", id, "removed"));
                }
                for output in event.get("outputs").and_then(Value::as_array).unwrap_or(&empty) {
                    out.extend(self.output(&s(output, "id"), &s(output, "state")));
                }
                out.extend(self.adbreak(event.get("ad")));
                out
            }
            _ => Vec::new(),
        }
    }

    fn source(&mut self, id: &str, state: &str) -> Vec<Delta> {
        if id.is_empty() || self.sources.get(id).map(String::as_str) == Some(state) {
            return Vec::new();
        }
        self.sources.insert(id.to_string(), state.to_string());
        vec![Delta::new("source", id, state)]
    }

    fn output(&mut self, id: &str, state: &str) -> Vec<Delta> {
        if id.is_empty() || self.outputs.get(id).map(String::as_str) == Some(state) {
            return Vec::new();
        }
        self.outputs.insert(id.to_string(), state.to_string());
        vec![Delta::new("output", id, state)]
    }

    /// An ad break is armed, then on air, then over. `on_air` is the field the
    /// status carries; the three names here are what a person would say.
    fn adbreak(&mut self, ad: Option<&Value>) -> Vec<Delta> {
        let to = match ad {
            None | Some(Value::Null) => None,
            Some(ad) => Some(match ad.get("on_air").and_then(Value::as_bool) {
                Some(true) => "on air".to_string(),
                _ => "armed".to_string(),
            }),
        };
        // Nothing to say about an ad break that was never armed.
        if to.is_none() && self.ad.is_none() {
            return Vec::new();
        }
        if to == self.ad {
            return Vec::new();
        }
        self.ad = to.clone();
        vec![Delta::new("adbreak", "", to.unwrap_or_else(|| "ended".into()))]
    }
}

/// One event as a delta, with no memory of what came before.
///
/// Kept for the cases that need no context, and for the tests. A whole session
/// goes through [`Tracker`].
pub fn from_event(event: &Value) -> Option<Delta> {
    Tracker::default().absorb(event).into_iter().next()
}

/// Compare two delta sequences and say what differs, in the shape a person
/// reads at the bottom of a failing test.
///
/// Compared per subject, not as one list. What `cam1` did is in order: it went
/// `connecting` and then `live`, and a run where it went `live` and then
/// `connecting` is a different run. What `cam1` did relative to what `cam2`
/// did is not in order: the two come from different pipelines on different
/// threads and which of them publishes first is a coin toss the machine
/// tosses. A test that failed on that would be a test nobody trusts, and a
/// flaky test is worse than no test.
///
/// The programme is the exception that is compared strictly, because the order
/// of what went on air is the whole story of a show.
pub fn diff(expected: &[Delta], produced: &[Delta]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut subjects: Vec<(String, String)> = Vec::new();
    for delta in expected.iter().chain(produced.iter()) {
        let key = (delta.what.clone(), delta.id.clone());
        if !subjects.contains(&key) {
            subjects.push(key);
        }
    }
    for (what, id) in subjects {
        let pick = |list: &[Delta]| -> Vec<String> {
            list.iter()
                .filter(|d| d.what == what && d.id == id)
                .map(|d| d.to.clone())
                .collect()
        };
        let want = pick(expected);
        let got = pick(produced);
        if want == got {
            continue;
        }
        let subject = if id.is_empty() { what.clone() } else { format!("{what} {id}") };
        lines.push(format!(
            "{subject}: expected [{}], got [{}]",
            want.join(", "),
            got.join(", ")
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_take_a_source_and_an_output_are_deltas_and_a_meter_is_not() {
        assert_eq!(
            from_event(&json!({ "type": "took", "source": "cam1", "at_running_time_ms": 1234 })),
            Some(Delta::new("program", "", "cam1"))
        );
        // A take to nothing is a take to the slate, and says so rather than
        // comparing as an empty string.
        assert_eq!(
            from_event(&json!({ "type": "took", "source": null })),
            Some(Delta::new("program", "", "slate"))
        );
        assert_eq!(
            from_event(&json!({ "type": "source_state_changed", "source": "cam1", "state": "live" })),
            Some(Delta::new("source", "cam1", "live"))
        );
        assert_eq!(
            from_event(&json!({ "type": "hook_blocked", "hook": "take.before", "plugin": "x", "reason": "y" })),
            Some(Delta::new("hook", "take.before", "blocked"))
        );
        // The things that change a hundred times a minute and decide nothing.
        assert_eq!(from_event(&json!({ "type": "audio_level", "peak_db": [-21.0] })), None);
        assert_eq!(from_event(&json!({ "type": "source_position", "source": "clip" })), None);
        assert_eq!(from_event(&json!({ "type": "status" })), None);
    }

    #[test]
    fn a_difference_names_the_subject_the_want_and_the_got() {
        let want = vec![Delta::new("program", "", "cam1"), Delta::new("program", "", "cam2")];
        let got = vec![Delta::new("program", "", "cam1"), Delta::new("program", "", "cam3")];
        let lines = diff(&want, &got);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "program: expected [cam1, cam2], got [cam1, cam3]");
        assert!(diff(&want, &want).is_empty());
    }

    #[test]
    fn what_one_source_did_is_ordered_and_two_sources_against_each_other_is_not() {
        let want = vec![
            Delta::new("source", "cam1", "connecting"),
            Delta::new("source", "cam2", "connecting"),
            Delta::new("source", "cam1", "live"),
        ];
        // The same three things, with the two sources interleaved the other
        // way round. Which pipeline publishes first is the machine's business.
        let got = vec![
            Delta::new("source", "cam1", "connecting"),
            Delta::new("source", "cam1", "live"),
            Delta::new("source", "cam2", "connecting"),
        ];
        assert!(diff(&want, &got).is_empty(), "{:?}", diff(&want, &got));

        // One source going live before it connected is a different run.
        let backwards = vec![
            Delta::new("source", "cam1", "live"),
            Delta::new("source", "cam1", "connecting"),
            Delta::new("source", "cam2", "connecting"),
        ];
        let lines = diff(&want, &backwards);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("source cam1:"), "{lines:?}");
    }

    #[test]
    fn a_short_run_and_a_long_one_both_show_up() {
        let want = vec![Delta::new("program", "", "cam1"), Delta::new("program", "", "cam2")];
        assert_eq!(diff(&want, &want[..1]), vec!["program: expected [cam1, cam2], got [cam1]"]);
        assert_eq!(diff(&want[..1], &want), vec!["program: expected [cam1], got [cam1, cam2]"]);
        // A subject that only one side has at all.
        let other = vec![Delta::new("source", "cam9", "failed")];
        assert_eq!(diff(&other, &[]), vec!["source cam9: expected [failed], got []"]);
    }

    #[test]
    fn expectations_load_as_a_document_or_as_a_bare_list() {
        let dir = std::env::temp_dir().join("gmx-expect-test");
        std::fs::create_dir_all(&dir).unwrap();
        let listed = dir.join("list.json");
        std::fs::write(&listed, r#"[{"what": "program", "to": "cam1"}]"#).unwrap();
        assert_eq!(Expectations::load(&listed).unwrap().changes.len(), 1);
        let doc = dir.join("doc.json");
        std::fs::write(&doc, r#"{"changes": [{"what": "program", "to": "cam1"}]}"#).unwrap();
        assert_eq!(Expectations::load(&doc).unwrap().changes[0].to, "cam1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
