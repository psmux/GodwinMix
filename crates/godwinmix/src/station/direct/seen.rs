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
}

impl Seen {
    pub fn input_live(&self) -> bool {
        self.input.as_ref().is_some_and(|v| v["state"] == "live")
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

    /// Take the rate the stats measure once the input has run a few
    /// seconds. `direct.input` carries the hub's first reading, of part of
    /// a second (none, or 8 for a 30 fps feed), and is not sent again for a
    /// rate. A fifth off or more is taken; a wobble is not. `STEADY`
    /// readings in a row must agree first: the seconds around a restarted
    /// sender read as 1 fps, or twice in a row as 10 fps for a 30 fps feed
    /// on a CI runner, and a rendition planned on that would carry it from
    /// then on. True when taken.
    pub fn settle_fps(&mut self) -> bool {
        const STEADY: u32 = 5;
        let measured = self.input_stats.as_ref().map(|s| s.fps).filter(|f| *f > 0.0);
        self.fps_run = match (measured, self.fps_run) {
            (Some(now), Some((then, n))) if (now - then).abs() < now / 5.0 => Some((then, n + 1)),
            (now, _) => now.map(|f| (f, 1)),
        };
        let agreed = self.fps_run.is_some_and(|(_, n)| n >= STEADY);
        let Some((input, fps)) = self.input.as_mut().zip(measured.filter(|_| agreed)) else { return false };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        if input["state"] != "live" || now.saturating_sub(input["since_ms"].as_u64().unwrap_or(now)) < 3000 {
            return false;
        }
        let video = &mut input["video"];
        let kept = video["fps"].as_f64().unwrap_or(0.0);
        if !video.is_object() || (kept - fps).abs() < fps / 5.0 {
            return false;
        }
        video["fps"] = serde_json::json!(fps);
        true
    }

    /// Whether every output in `ids` is live.
    pub fn all_live(&self, ids: &[String]) -> bool {
        ids.iter().all(|id| self.outputs.get(id).is_some_and(|(l, _)| l.state == DestinationState::Live))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn a_first_reading_of_the_rate_gives_way_to_the_measured_one_and_a_wobble_does_not() {
        let input = Some(json!({"state": "live", "since_ms": 1000, "video": {"codec": "h264", "fps": 8.0}}));
        let mut seen = Seen { input, ..Seen::default() };
        assert!(!seen.settle_fps(), "nothing measured yet");
        for fps in [29.97, 30.01, 29.98, 30.0] {
            seen.take_stats(&json!({"id": "a", "input": {"fps": fps}}));
            assert!(!seen.settle_fps(), "{fps} is not a steady rate yet");
        }
        seen.take_stats(&json!({"id": "a", "input": {"fps": 29.99}}));
        assert!(seen.settle_fps());
        assert_eq!(seen.input.as_ref().unwrap()["video"]["fps"], 29.99);
        seen.take_stats(&json!({"id": "a", "input": {"fps": 30.02}}));
        assert!(!seen.settle_fps(), "a wobble is not a new rate");
        seen.input = Some(json!({"state": "live", "since_ms": u64::MAX, "video": {"fps": 0}}));
        assert!(!seen.settle_fps(), "an input live for less than a few seconds is left alone");
    }

    #[test]
    fn the_second_a_sender_restarts_in_is_not_taken_for_its_rate() {
        let input = Some(json!({"state": "live", "since_ms": 1000, "video": {"codec": "h264", "fps": 30.0}}));
        let mut seen = Seen { input, ..Seen::default() };
        for fps in [30.0, 0.87, 30.0, 29.9, 10.1, 9.8, 0.42, 10.0, 10.2] {
            seen.take_stats(&json!({"id": "a", "input": {"fps": fps}}));
            assert!(!seen.settle_fps(), "{fps}");
        }
        assert_eq!(seen.input.as_ref().unwrap()["video"]["fps"], 30.0);
    }
}
