//! What the direct host last said about one show.

use godwinmix_protocol::destination::{DestinationLive, DestinationState};
use godwinmix_protocol::shows::{AlarmKind, Health, InputStats, OutputStats};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct Seen {
    /// The last `direct.input`, as the host sent it.
    pub input: Option<Value>,
    /// Each output's last `direct.output`, and when its state began.
    pub outputs: BTreeMap<String, (DestinationLive, Instant)>,
    /// The host's own alarms, from `direct.health`.
    pub host_health: Option<Health>,
    /// The show's row of the last `direct.stats`.
    pub input_stats: Option<InputStats>,
    pub output_stats: Vec<OutputStats>,
    /// The health last sent as `event/show.health`.
    pub announced: Option<Health>,
    /// A show that composites: the hub path its input source was given.
    pub source_for: Option<String>,
    /// When each alarm that holds now was first seen, unix ms. Kept until
    /// the alarm clears, so its age reads true.
    pub alarm_since: BTreeMap<AlarmKind, u64>,
    /// A show that composites: what it last said of its own programme over
    /// the link. Dropped when the link closes, so a dead show never reads
    /// as the last health it sent.
    pub show_health: Option<Health>,
    /// A show that composites: when its link closed or its process failed,
    /// unix milliseconds, until it says hello again or is started afresh.
    pub lost_ms: Option<u64>,
    /// The first frame rate of the current run of readings that agree with
    /// it, and how many readings the run holds.
    pub fps_run: Option<(f64, u32)>,
    /// The rate the stats settled, and the picture (codec and size) it was
    /// settled for. See `rate.rs`.
    pub settled: Option<(Value, f64)>,
}

impl Seen {
    pub fn input_live(&self) -> bool {
        self.input.as_ref().is_some_and(|v| v["state"] == "live")
    }

    /// The host has said what the input is doing and it is not live. Not
    /// heard of yet is not down: a show just started has said nothing.
    pub fn input_down(&self) -> bool {
        self.input.as_ref().is_some_and(|v| v["state"] != "live")
    }

    /// Where a hub reader asks for the input, when the host said.
    pub fn relay(&self) -> Option<(String, String)> {
        let v = self.input.as_ref()?;
        let relay = v["relay"].as_str().filter(|s| !s.is_empty())?;
        let stream = v["stream"].as_str().filter(|s| !s.is_empty())?;
        Some((relay.to_string(), stream.to_string()))
    }

    /// An output as a view shows it: off is off, unheard of is waiting.
    pub fn output(&self, id: &str, enabled: bool) -> DestinationLive {
        if !enabled {
            return DestinationLive::default();
        }
        match self.outputs.get(id) {
            Some((live, since)) => DestinationLive { since_ms: since.elapsed().as_millis() as u64, ..live.clone() },
            None => DestinationLive { state: DestinationState::Waiting, ..Default::default() },
        }
    }

    /// Take a `direct.output` report. Answers whether its state, error or
    /// reconnect count moved.
    pub fn take_output(&mut self, id: &str, v: &Value) -> bool {
        let Ok(live) = serde_json::from_value::<DestinationLive>(v.clone()) else { return false };
        let since = Instant::now().checked_sub(Duration::from_millis(live.since_ms)).unwrap_or_else(Instant::now);
        match self.outputs.get_mut(id) {
            Some((was, began)) => {
                let moved = was.state != live.state || was.error != live.error || was.reconnects != live.reconnects;
                if was.state != live.state {
                    *began = since;
                }
                *was = live;
                moved
            }
            None => {
                self.outputs.insert(id.to_string(), (live, since));
                true
            }
        }
    }

    /// Take a show's row of `direct.stats`.
    pub fn take_stats(&mut self, row: &Value) {
        self.input_stats = serde_json::from_value(row["input"].clone()).ok();
        self.output_stats = serde_json::from_value(row["outputs"].clone()).unwrap_or_default();
        for o in &self.output_stats {
            if let Some((live, _)) = self.outputs.get_mut(&o.id) {
                live.kbps = o.kbps;
            }
        }
    }

    /// Whether every output in `ids` is live.
    pub fn all_live(&self, ids: &[String]) -> bool {
        ids.iter().all(|id| self.outputs.get(id).is_some_and(|(l, _)| l.state == DestinationState::Live))
    }
}

/// An output of a direct show as it is while the input is down. The HLS
/// packager, and an output holding its connection open, go on saying live
/// with the last bitrate they counted after the input stops: the headend's
/// output table read "live 2.1 Mb/s" for a channel whose input had gone.
/// Nothing goes out of a direct show while its input is down, so a live
/// output waits, with that as the reason, and none of them counts a rate.
pub fn behind_input(live: DestinationLive, input_down: bool) -> DestinationLive {
    if !input_down {
        return live;
    }
    if live.state == DestinationState::Live {
        return DestinationLive { state: DestinationState::Waiting, kbps: 0, error: Some(INPUT_DOWN.into()), ..live };
    }
    DestinationLive { kbps: 0, ..live }
}

/// The reason [`behind_input`] gives.
pub const INPUT_DOWN: &str = "the input is not live, so nothing is sent";

/// The log line for an output whose state or error moved: a warning while
/// it is down with a reason, and a note when it goes live or waits. None
/// when neither moved, so a steady output writes nothing. The tiles said
/// why an output was down and the station's log said nothing at all.
pub fn told(was: Option<&DestinationLive>, now: &DestinationLive) -> Option<(bool, String)> {
    if was.is_some_and(|w| w.state == now.state && w.error == now.error) {
        return None;
    }
    let state = serde_json::to_value(now.state).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
    let down = matches!(now.state, DestinationState::Reconnecting | DestinationState::Failed);
    let text = match &now.error {
        Some(why) => format!("{state}: {why}"),
        None => state,
    };
    Some((down && now.error.is_some(), text))
}

/// Write [`told`]'s line, if any, for one output of one show.
pub fn log_output(show: &str, output: &str, was: Option<&DestinationLive>, now: &DestinationLive) {
    match told(was, now) {
        Some((true, text)) => tracing::warn!(show, output, reconnects = now.reconnects, "output {text}"),
        Some((false, text)) => tracing::info!(show, output, "output {text}"),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_output_is_logged_when_its_state_or_error_moves_and_not_otherwise() {
        let down = |error: &str| DestinationLive { state: DestinationState::Reconnecting, reconnects: 3, error: Some(error.into()), ..Default::default() };
        let lost = down("nothing came from the relay for 30 s");
        let (warn, text) = told(None, &lost).expect("a first report is told");
        assert!(warn, "down with a reason is a warning");
        assert_eq!(text, "reconnecting: nothing came from the relay for 30 s");
        assert!(told(Some(&lost), &DestinationLive { reconnects: 4, ..lost.clone() }).is_none(), "one more reconnect alone is not a line");
        assert!(told(Some(&lost), &down("the relay at 127.0.0.1:1935 did not answer")).is_some(), "a new reason is");
        let live = DestinationLive { state: DestinationState::Live, ..Default::default() };
        assert_eq!(told(Some(&lost), &live), Some((false, "live".into())));
    }

    #[test]
    fn an_output_follows_the_input_down_and_back() {
        let live = DestinationLive { state: DestinationState::Live, kbps: 2100, ..Default::default() };
        assert_eq!(behind_input(live.clone(), false), live, "a live input changes nothing");
        let held = behind_input(live.clone(), true);
        assert_eq!(held.state, DestinationState::Waiting);
        assert_eq!(held.kbps, 0, "no rate is counted with nothing coming in");
        assert_eq!(held.error.as_deref(), Some(INPUT_DOWN));
        let failing = DestinationLive { state: DestinationState::Reconnecting, kbps: 40, error: Some("refused".into()), ..Default::default() };
        let still = behind_input(failing, true);
        assert_eq!((still.state, still.kbps, still.error.as_deref()), (DestinationState::Reconnecting, 0, Some("refused")), "its own reason stays");

        let mut seen = Seen::default();
        assert!(!seen.input_down(), "an input not heard of yet is not down");
        seen.input = Some(json!({"state": "live"}));
        assert!(!seen.input_down());
        seen.input = Some(json!({"state": "idle"}));
        assert!(seen.input_down());
    }

    #[test]
    fn an_output_report_moves_only_on_state_error_or_reconnects() {
        let mut seen = Seen::default();
        assert_eq!(seen.output("yt", true).state, DestinationState::Waiting);
        assert!(seen.take_output("yt", &json!({"state": "connecting", "since_ms": 0, "kbps": 0, "reconnects": 0, "error": null})));
        assert!(seen.take_output("yt", &json!({"state": "live", "since_ms": 0, "kbps": 0, "reconnects": 0, "error": null})));
        assert!(!seen.take_output("yt", &json!({"state": "live", "since_ms": 0, "kbps": 900, "reconnects": 0, "error": null})));
        seen.take_stats(&json!({"id": "a", "input": {"kbps": 5000, "fps": 25.0}, "outputs": [{"id": "yt", "kbps": 4800}]}));
        assert_eq!(seen.output("yt", true).kbps, 4800);
        assert_eq!(seen.input_stats.as_ref().map(|i| i.kbps), Some(5000));
        assert!(seen.all_live(&["yt".into()]));
        assert_eq!(seen.output("yt", false).state, DestinationState::Off);
    }
}
