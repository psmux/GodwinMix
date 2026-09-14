//! The state store: a snapshot, then deltas, read at flush.
//!
//! The core sends `event/snapshot`, then deltas, then `event/flush`. A surface
//! reads from here and never re-reads the wire, and it repaints at flush and
//! not per event, which is why a batch of twenty changes is one repaint.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::generated::{
    AlertEvent, Event, Meters, MixerStatus, MultiviewLayout, OutputStatus, SourceStatus,
};

/// Everything a surface draws from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The last sequence number the core flushed.
    pub seq: u64,
    pub connected: bool,
    pub status: MixerStatus,
    /// The preview scene, once a core sends `event/preview.changed`.
    pub preview: Option<String>,
    /// Source id to "program", "preview" or "off", from `event/tally`.
    pub tally: BTreeMap<String, String>,
    /// Peak dBFS, from `event/meters`. Ten a second, and outside the flush path.
    pub meters: Meters,
    /// The newest first, capped at fifty.
    pub alerts: Vec<AlertEvent>,
    /// The mosaic grid the binary frames are cut to.
    pub layout: Option<MultiviewLayout>,
}

impl State {
    pub fn program(&self) -> Option<&str> {
        self.status.program.as_deref()
    }

    pub fn source(&self, id: &str) -> Option<&SourceStatus> {
        self.status.sources.iter().find(|s| s.id == id)
    }

    pub fn output(&self, id: &str) -> Option<&OutputStatus> {
        self.status.outputs.iter().find(|o| o.id == id)
    }

    /// "program", "preview" or "off" for one source.
    ///
    /// Answered from `event/tally` when the client asked for it, and worked out
    /// from the programme otherwise, so a surface that declined the tally
    /// stream still colours its buttons.
    pub fn tally_of(&self, id: &str) -> &str {
        if let Some(t) = self.tally.get(id) {
            return t.as_str();
        }
        if self.program() == Some(id) {
            return "program";
        }
        if self.preview.as_deref() == Some(id) {
            return "preview";
        }
        "off"
    }

    /// Fold one event in. Returns true when the event ended a batch, which is
    /// the moment a surface repaints.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::Snapshot(snap) => {
                self.status = snap.state.clone();
                self.seq = snap.seq;
                self.connected = true;
            }
            Event::Flush(flush) => {
                self.seq = flush.seq;
                return true;
            }
            Event::ProgramTook(took) => {
                if took.source.is_some() || took.scene.is_some() {
                    self.status.program = took.source.clone().or_else(|| took.scene.clone());
                } else {
                    self.status.program = None;
                }
            }
            Event::SourceState(ev) => self.patch_source(ev),
            Event::OutputState(ev) => self.patch_output(ev),
            Event::AdbreakChanged(ev) => self.status.ad = ev.ad.clone(),
            Event::Meters(m) => self.meters = m.clone(),
            Event::Tally(t) => {
                self.tally = t
                    .sources
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect();
            }
            Event::MultiviewLayout(layout) => self.layout = Some(layout.clone()),
            Event::Alert(alert) => {
                self.alerts.insert(0, alert.clone());
                self.alerts.truncate(50);
            }
            Event::Other { name, params } if name == "preview.changed" => {
                self.preview = params.get("scene").and_then(Value::as_str).map(str::to_string);
            }
            _ => {}
        }
        false
    }

    fn patch_source(&mut self, ev: &crate::generated::SourceStateEvent) {
        let Some(id) = ev.source.as_deref() else { return };
        if let Some(row) = self.status.sources.iter_mut().find(|s| s.id == id) {
            if let Some(state) = ev.state.clone() {
                row.state = state;
            }
        }
    }

    fn patch_output(&mut self, ev: &crate::generated::OutputStateEvent) {
        let Some(id) = ev.output.as_deref() else { return };
        if let Some(row) = self.status.outputs.iter_mut().find(|o| o.id == id) {
            if let Some(state) = ev.state.clone() {
                row.state = state;
            }
            if let Some(n) = ev.reconnects {
                row.reconnects = n as u32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot_event() -> Event {
        Event::parse(
            "event/snapshot",
            json!({
                "seq": 4,
                "state": {
                    "program": "cam1",
                    "sources": [
                        {"id": "cam1", "name": "Camera 1", "uri": "rtmp://x", "state": "live",
                         "has_video": true, "has_audio": true},
                        {"id": "cam2", "name": "Camera 2", "uri": "rtmp://y", "state": "connecting",
                         "has_video": true, "has_audio": false}
                    ],
                    "outputs": [],
                    "backend": {},
                    "multiview": {},
                    "running_time_ms": 100,
                    "uptime_secs": 3
                }
            }),
        )
    }

    #[test]
    fn snapshot_then_deltas_then_flush() {
        let mut state = State::default();
        assert!(!state.apply(&snapshot_event()));
        assert_eq!(state.program(), Some("cam1"));
        assert_eq!(state.source("cam2").map(|s| s.state.as_str()), Some("connecting"));

        let delta = Event::parse("event/source.state", json!({"source": "cam2", "state": "live"}));
        assert!(!state.apply(&delta));
        assert_eq!(state.source("cam2").map(|s| s.state.as_str()), Some("live"));

        let took = Event::parse("event/program.took", json!({"source": "cam2"}));
        state.apply(&took);
        assert_eq!(state.program(), Some("cam2"));

        assert!(state.apply(&Event::parse("event/flush", json!({"seq": 9}))));
        assert_eq!(state.seq, 9);
    }

    #[test]
    fn tally_falls_back_to_the_programme() {
        let mut state = State::default();
        state.apply(&snapshot_event());
        assert_eq!(state.tally_of("cam1"), "program");
        assert_eq!(state.tally_of("cam2"), "off");
        state.apply(&Event::parse("event/tally", json!({"sources": {"cam2": "preview"}})));
        assert_eq!(state.tally_of("cam2"), "preview");
    }

    #[test]
    fn an_unknown_event_is_kept_not_dropped() {
        let event = Event::parse("event/something.new", json!({"x": 1}));
        match event {
            Event::Other { name, params } => {
                assert_eq!(name, "something.new");
                assert_eq!(params["x"], 1);
            }
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn alerts_stack_newest_first_and_stop_at_fifty() {
        let mut state = State::default();
        for i in 0..60 {
            state.apply(&Event::parse(
                "event/alert",
                json!({"severity": "warning", "message": format!("alert {i}")}),
            ));
        }
        assert_eq!(state.alerts.len(), 50);
        assert_eq!(state.alerts[0].message.as_deref(), Some("alert 59"));
    }
}
