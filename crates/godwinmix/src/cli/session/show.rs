//! `gmx session show`: a session log as a timeline a person reads.
//!
//! The file is JSONL because a machine reads it. This is the other half: one
//! line per thing that happened, in the style of the supervisor's own
//! decisions, which say "cam1 rebuilt: no buffers for 10 s, video was 2,427 ms
//! in the programme's future" rather than "cam1 rebuilt".

use super::Record;
use serde_json::Value;
use std::fmt::Write as _;

/// Render a timeline between two timestamp prefixes.
///
/// `from` and `to` are compared as prefixes of the RFC 3339 time, so
/// `--from 20:13` works without anybody typing a whole timestamp. That is the
/// form the loop in 10 section 5 uses.
pub fn timeline(records: &[Record], from: Option<&str>, to: Option<&str>, all: bool) -> String {
    let mut out = String::new();
    let mut shown = 0usize;
    let mut folded = 0usize;
    // The whole status document is published whenever the shape of the show
    // changes, and a source going live is that document twice with one field
    // different. The tracker turns those into the one line a person wants.
    let mut tracker = super::delta::Tracker::default();
    for record in records {
        if !in_range(&record.ts, from, to) {
            continue;
        }
        // Four kinds of event say the same things in different words: a
        // status document, and the three deltas that are also folded into one.
        // All four go through the tracker, which is what stops a source that
        // failed once from being reported eleven times.
        const TRACKED: &[&str] =
            &["status", "source_state_changed", "output_state_changed", "ad_break_changed"];
        if record.event_type().is_some_and(|t| TRACKED.contains(&t)) {
            let changes = record.event().map(|e| tracker.absorb(e)).unwrap_or_default();
            if changes.is_empty() {
                folded += 1;
                continue;
            }
            for change in changes {
                shown += 1;
                let _ = writeln!(out, "{} {}", clock(&record.ts), sentence(&change));
            }
            continue;
        }
        // Keep the tracker current on the rest, so a take it has already seen
        // is not repeated by the status that follows it.
        if let Some(event) = record.event() {
            let _ = tracker.absorb(event);
        }
        match line(record) {
            Some(text) => {
                shown += 1;
                let _ = writeln!(out, "{} {text}", clock(&record.ts));
            }
            None if all => {
                shown += 1;
                let _ = writeln!(out, "{} {} {}", clock(&record.ts), record.kind, record.value);
            }
            None => folded += 1,
        }
    }
    if shown == 0 {
        let _ = writeln!(out, "nothing in that range. The log holds {} record(s).", records.len());
        return out;
    }
    if folded > 0 {
        let _ = writeln!(out, "({folded} record(s) folded away; --all shows them)");
    }
    out
}

/// A state delta as a sentence.
fn sentence(change: &super::delta::Delta) -> String {
    match change.what.as_str() {
        "source" if change.to == "removed" => format!("source {} was removed", change.id),
        "source" => format!("source {} is {}", change.id, change.to),
        "output" => format!("output {} is {}", change.id, change.to),
        "adbreak" => format!("ad break {}", change.to),
        _ => change.line(),
    }
}

/// `2026-09-14T20:13:58.402Z` as `20:13:58.402`, because a session is one
/// evening and the date is in the file name.
fn clock(ts: &str) -> &str {
    ts.split('T').nth(1).map(|t| t.trim_end_matches('Z')).unwrap_or(ts)
}

/// Whether a timestamp is inside the range, by prefix.
fn in_range(ts: &str, from: Option<&str>, to: Option<&str>) -> bool {
    let time = clock(ts);
    if let Some(from) = from {
        if time < from && !time.starts_with(from) {
            return false;
        }
    }
    if let Some(to) = to {
        if time > to && !time.starts_with(to) {
            return false;
        }
    }
    true
}

/// One record as a sentence, or nothing when it is noise.
fn line(record: &Record) -> Option<String> {
    let s = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or("?").to_string();
    match record.kind.as_str() {
        "command" => {
            let method = record.method().unwrap_or("?");
            let who = record.value.get("token_id").and_then(Value::as_str).unwrap_or("anonymous");
            let params = record.params();
            let detail = summarise(method, &params);
            Some(match detail.is_empty() {
                true => format!("{who} called {method}"),
                false => format!("{who} called {method} {detail}"),
            })
        }
        "event" => {
            let event = record.event()?;
            match event.get("type").and_then(Value::as_str)? {
                "took" => Some(format!(
                    "programme -> {} at {} ms",
                    event.get("source").and_then(Value::as_str).unwrap_or("slate"),
                    event.get("at_running_time_ms").and_then(Value::as_u64).unwrap_or(0)
                )),
                // source.state, output.state and adbreak.changed never reach
                // here: the tracker above renders them, deduplicated.
                "alert" => {
                    Some(format!("{}: {}", s(event, "severity"), s(event, "message")))
                }
                "hook_blocked" => Some(format!(
                    "hook {} ({}) did not get its say: {}",
                    s(event, "hook"),
                    s(event, "plugin"),
                    s(event, "reason")
                )),
                "media_changed" => Some(format!("media {} changed", s(event, "name"))),
                "ui_changed" => Some("the surface defaults changed".to_string()),
                _ => None,
            }
        }
        "decision" => Some(format!(
            "{} {}: {} {}",
            s(&record.value, "instance"),
            s(&record.value, "decision"),
            s(&record.value, "why"),
            record.value.get("numbers").map(|n| n.to_string()).unwrap_or_default()
        )),
        "gap" => Some(format!(
            "the recorder fell behind and missed {} record(s)",
            record.value.get("missed").and_then(Value::as_u64).unwrap_or(0)
        )),
        _ => None,
    }
}

/// The one or two fields of a command worth putting on the line.
fn summarise(method: &str, params: &Value) -> String {
    let get = |key: &str| params.get(key).and_then(Value::as_str).map(str::to_string);
    match method {
        "program.take" => get("source").unwrap_or_else(|| "the slate".into()),
        "source.add" => {
            let id = get("id").unwrap_or_default();
            let uri = get("uri").unwrap_or_default();
            format!("{id} {uri}").trim().to_string()
        }
        "source.remove" | "output.remove" | "plugin.remove" => get("id").unwrap_or_default(),
        "output.add" => get("url").or_else(|| get("id")).unwrap_or_default(),
        "adbreak.start" => get("media").unwrap_or_default(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn records() -> Vec<Record> {
        super::super::parse(&[
            json!({"seq":0,"ts":"2026-09-14T20:13:00.000Z","kind":"command","method":"source.add","token_id":"desk","params":{"id":"cam1","uri":"test://smpte"}}).to_string(),
            json!({"seq":1,"ts":"2026-09-14T20:13:01.000Z","kind":"event","event":{"type":"source_state_changed","source":"cam1","state":"live"}}).to_string(),
            json!({"seq":2,"ts":"2026-09-14T20:13:58.402Z","kind":"command","method":"program.take","token_id":"desk","params":{"source":"cam1"}}).to_string(),
            json!({"seq":3,"ts":"2026-09-14T20:13:58.410Z","kind":"event","event":{"type":"took","source":"cam1","at_running_time_ms":58402}}).to_string(),
            json!({"seq":4,"ts":"2026-09-14T20:14:08.000Z","kind":"decision","instance":"cam1","decision":"rebuild","why":"no buffers for 10 s","numbers":{"behind_ms":-2427}}).to_string(),
            json!({"seq":5,"ts":"2026-09-14T20:14:09.000Z","kind":"event","event":{"type":"audio_level","peak_db":[-21.0]}}).to_string(),
        ].join("\n"))
    }

    #[test]
    fn a_timeline_reads_as_sentences_and_folds_the_noise_away() {
        let text = timeline(&records(), None, None, false);
        assert!(text.contains("20:13:00.000 desk called source.add cam1 test://smpte"), "{text}");
        assert!(text.contains("20:13:58.410 programme -> cam1 at 58402 ms"), "{text}");
        assert!(text.contains("cam1 rebuild: no buffers for 10 s"), "{text}");
        assert!(text.contains("behind_ms"), "the numbers are on the line: {text}");
        assert!(!text.contains("audio_level"), "{text}");
        assert!(text.contains("source cam1 is live"), "a status becomes a sentence: {text}");
        assert!(text.contains("record(s) folded away"), "{text}");
    }

    #[test]
    fn all_shows_the_records_the_timeline_folds() {
        let text = timeline(&records(), None, None, true);
        assert!(text.contains("audio_level"), "{text}");
    }

    #[test]
    fn a_range_is_given_as_a_prefix_of_the_time() {
        let text = timeline(&records(), Some("20:13:58"), None, false);
        assert!(text.contains("programme -> cam1"), "{text}");
        assert!(!text.contains("source.add"), "{text}");

        let text = timeline(&records(), None, Some("20:13:01"), false);
        assert!(text.contains("source.add"), "{text}");
        assert!(!text.contains("programme -> cam1"), "{text}");

        let empty = timeline(&records(), Some("23:00"), None, false);
        assert!(empty.contains("nothing in that range"), "{empty}");
    }
}
