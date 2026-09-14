//! The state store: a snapshot, then deltas, painted only at `event/flush`.
//!
//! The rule from 05 section 2 is that a client renders at the flush and never
//! before, so it never paints half an update. That is what the two copies here
//! are for: deltas land on `staged`, and the flush promotes `staged` to `live`
//! in one move. `live` is the only thing the screen ever reads.
//!
//! Promoting is a clone of a handful of source and output records, which on
//! the largest grid the core builds is under a kilobyte. Sharing the
//! allocation instead would buy nothing and cost a lifetime on every widget.

use crate::model::{
    utc_hms, Alert, MixerStatus, Meters, MultiviewLayout, OutputState, SourceState,
};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many alerts are kept. The brief's number, and about a screen of them.
pub const ALERT_LIMIT: usize = 50;

/// Everything the screen draws, as of one flush.
#[derive(Debug, Clone, Default)]
pub struct View {
    pub status: MixerStatus,
    /// Source id to "program", "preview" or "off", from `event/tally`. Empty
    /// until the first one arrives, and the screen falls back to the
    /// programme id in that window.
    pub tally: BTreeMap<String, String>,
    pub meters: Meters,
    pub alerts: VecDeque<Alert>,
    pub layout: Option<MultiviewLayout>,
    /// False until the first `event/snapshot`, so the screen can say it is
    /// waiting rather than draw an empty mixer.
    pub ready: bool,
}

impl View {
    pub fn source(&self, id: &str) -> Option<&crate::model::SourceStatus> {
        self.status.sources.iter().find(|s| s.id == id)
    }

    /// What the tally says about one source, falling back to the programme id
    /// while `ext.tally` has not produced its first message.
    pub fn tally_of(&self, id: &str) -> &str {
        if let Some(state) = self.tally.get(id) {
            return state;
        }
        if self.status.program.as_deref() == Some(id) {
            "program"
        } else {
            "off"
        }
    }
}

/// What one incoming event did to the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Absorbed into the staged view. Nothing to draw yet.
    Staged,
    /// A flush: `live` moved on and the screen should paint.
    Render,
    /// The core says this client fell behind. Subscribe again.
    Resync,
    /// Not an event this UI reads.
    Ignored,
}

#[derive(Debug, Default)]
pub struct Store {
    live: View,
    staged: View,
    /// The last sequence number seen, from the flush or from any event
    /// carrying one.
    pub seq: u64,
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    /// The view the screen draws. Only a flush moves it.
    pub fn view(&self) -> &View {
        &self.live
    }

    /// Thrown away and rebuilt on every re-subscribe, because a fresh
    /// subscription is a fresh snapshot and anything half applied belongs to
    /// the connection that went away.
    pub fn reset(&mut self) {
        let alerts = std::mem::take(&mut self.live.alerts);
        *self = Self::default();
        // Alerts are the operator's history of the session, not part of the
        // mixer's state, so a reconnect does not wipe them.
        self.live.alerts = alerts.clone();
        self.staged.alerts = alerts;
    }

    /// One JSON-RPC notification from the core.
    pub fn apply(&mut self, method: &str, params: &Value) -> Outcome {
        if let Some(seq) = params.get("seq").and_then(Value::as_u64) {
            self.seq = seq;
        }
        let name = method.strip_prefix("event/").unwrap_or(method);
        match name {
            "snapshot" => self.snapshot(params),
            "flush" => {
                self.live = self.staged.clone();
                Outcome::Render
            }
            "resync" => Outcome::Resync,
            "program.took" => self.took(params),
            "source.state" => self.source_state(params),
            "source.position" => self.source_position(params),
            "output.state" => self.output_state(params),
            "adbreak.changed" => {
                self.staged.status.ad = params
                    .get("ad")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or(None);
                Outcome::Staged
            }
            "meters" => {
                if let Ok(m) = serde_json::from_value::<Meters>(params.clone()) {
                    self.staged.meters = m;
                }
                Outcome::Staged
            }
            "tally" => {
                if let Some(map) = params.get("sources").and_then(Value::as_object) {
                    self.staged.tally = map
                        .iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                        .collect();
                }
                Outcome::Staged
            }
            "alert" => self.alert(params),
            "multiview.layout" => {
                if let Ok(l) = serde_json::from_value::<MultiviewLayout>(params.clone()) {
                    self.staged.layout = Some(l);
                }
                Outcome::Staged
            }
            _ => Outcome::Ignored,
        }
    }

    /// A local note in the alert list, for things that happened to this client
    /// rather than to the mixer: the link dropped, the link came back.
    pub fn note(&mut self, severity: &str, message: String) {
        let alert = Alert { severity: severity.to_string(), message, at: now_hms() };
        push_alert(&mut self.staged.alerts, alert.clone());
        push_alert(&mut self.live.alerts, alert);
    }

    fn snapshot(&mut self, params: &Value) -> Outcome {
        let Some(state) = params.get("state") else { return Outcome::Ignored };
        match serde_json::from_value::<MixerStatus>(state.clone()) {
            Ok(status) => {
                self.staged.status = status;
                self.staged.ready = true;
                Outcome::Staged
            }
            Err(_) => Outcome::Ignored,
        }
    }

    fn took(&mut self, params: &Value) -> Outcome {
        let source = params.get("source").and_then(Value::as_str).map(str::to_string);
        self.staged.status.program = source;
        if let Some(ms) = params.get("at_running_time_ms").and_then(Value::as_u64) {
            self.staged.status.running_time_ms = ms;
        }
        Outcome::Staged
    }

    fn source_state(&mut self, params: &Value) -> Outcome {
        let Some(id) = params.get("source").and_then(Value::as_str) else { return Outcome::Ignored };
        let state: SourceState = params
            .get("state")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        if let Some(src) = self.staged.status.sources.iter_mut().find(|s| s.id == id) {
            src.state = state;
        }
        Outcome::Staged
    }

    fn source_position(&mut self, params: &Value) -> Outcome {
        let Some(id) = params.get("source").and_then(Value::as_str) else { return Outcome::Ignored };
        if let Some(src) = self.staged.status.sources.iter_mut().find(|s| s.id == id) {
            src.position_ms = params.get("position_ms").and_then(Value::as_u64);
            if let Some(d) = params.get("duration_ms").and_then(Value::as_u64) {
                src.duration_ms = Some(d);
            }
        }
        Outcome::Staged
    }

    fn output_state(&mut self, params: &Value) -> Outcome {
        let Some(id) = params.get("output").and_then(Value::as_str) else { return Outcome::Ignored };
        let state: OutputState = params
            .get("state")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        if let Some(out) = self.staged.status.outputs.iter_mut().find(|o| o.id == id) {
            out.state = state;
            if let Some(n) = params.get("reconnects").and_then(Value::as_u64) {
                out.reconnects = n as u32;
            }
        }
        Outcome::Staged
    }

    fn alert(&mut self, params: &Value) -> Outcome {
        let message =
            params.get("message").and_then(Value::as_str).unwrap_or_default().to_string();
        if message.is_empty() {
            return Outcome::Ignored;
        }
        let severity =
            params.get("severity").and_then(Value::as_str).unwrap_or("info").to_string();
        push_alert(&mut self.staged.alerts, Alert { severity, message, at: now_hms() });
        Outcome::Staged
    }
}

fn push_alert(list: &mut VecDeque<Alert>, alert: Alert) {
    list.push_front(alert);
    while list.len() > ALERT_LIMIT {
        list.pop_back();
    }
}

fn now_hms() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    utc_hms(secs)
}
