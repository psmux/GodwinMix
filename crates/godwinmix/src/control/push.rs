//! `event/telemetry` and `event/agent.state`, pushed to a client that asked.
//!
//! Two `ext` keys from 03 section 6. `telemetry` turns on the probes and sends
//! a line of numbers at 1 to 10 per second. `agent` sends the whole
//! `agent.state` document when a threshold crosses or a state flips, so an
//! agent stops polling and still hears about a source going black within a
//! tick.
//!
//! Nothing here runs unless a client asks. Holding the telemetry lease is what
//! keeps the probes measuring, and dropping this drops the lease.

use godwinmix_protocol::requests::{AgentExt, Ext};
use godwinmix_core::telemetry::{self, Lease};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use tokio::time::Instant as TokioInstant;

/// A client's own push state. One per `/rpc` connection.
pub struct Push {
    /// Holding this is what keeps the probes running.
    lease: Option<Lease<'static>>,
    /// Ticks per second, 1 to 10, when telemetry is wanted.
    hz: u32,
    last_tick: Option<Instant>,
    thresholds: Option<Thresholds>,
    /// What the last pushed `agent.state` was triggered by and when, so the
    /// same condition does not fire every tick.
    last_agent: Option<Instant>,
    /// The threshold conditions that were true at the last push, so a push
    /// happens on the edge rather than for as long as the condition lasts.
    crossed: Crossed,
    /// What the client was last told is on air, so a flip is noticed without
    /// asking the mixer.
    program: Option<String>,
    agent_wanted: bool,
}

/// `ext.agent` thresholds, with the defaults from 09 section 5 item 12.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub shot: f64,
    pub black: f64,
    pub freeze_ms: u64,
    pub silence_ms: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self { shot: 0.3, black: 0.98, freeze_ms: 200, silence_ms: 500 }
    }
}

impl From<&AgentExt> for Thresholds {
    fn from(ext: &AgentExt) -> Self {
        let d = Self::default();
        match ext {
            AgentExt::On(_) => d,
            AgentExt::Thresholds { shot, black, freeze_ms, silence_ms } => Self {
                shot: shot.unwrap_or(d.shot),
                black: black.unwrap_or(d.black),
                freeze_ms: freeze_ms.unwrap_or(d.freeze_ms),
                silence_ms: silence_ms.unwrap_or(d.silence_ms),
            },
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Crossed {
    shot: bool,
    black: bool,
    freeze: bool,
    silence: bool,
}

/// At most one pushed document a second, whatever is happening. A picture that
/// flickers must not become a message loop.
const AGENT_MIN_GAP: Duration = Duration::from_secs(1);

impl Default for Push {
    fn default() -> Self {
        Self::none()
    }
}

impl Push {
    pub fn none() -> Self {
        Self {
            lease: None,
            hz: 0,
            last_tick: None,
            thresholds: None,
            last_agent: None,
            crossed: Crossed::default(),
            program: None,
            agent_wanted: false,
        }
    }

    /// Take up whatever this `core.subscribe` asked for, and let go of
    /// whatever it did not. Re-subscribing without `telemetry` stops the
    /// probes, which is the point of the lease.
    pub fn configure(&mut self, ext: &Ext) {
        let hz = ext.telemetry_hz();
        let wants = hz.is_some() || ext.wants_agent();
        self.hz = hz.unwrap_or(0);
        self.thresholds = ext.wants_agent().then(|| Thresholds::from(ext.agent.as_ref().unwrap()));
        self.agent_wanted = ext.wants_agent();
        // The agent push reads the same numbers, so it needs the probes too.
        if wants {
            if self.lease.is_none() {
                self.lease = Some(telemetry::telemetry().lease());
            }
        } else {
            self.lease = None;
        }
        // Start the clock now, so the first tick is one period away rather
        // than never: `next_due` has nothing to count from otherwise.
        self.last_tick = wants.then(Instant::now);
        self.last_agent = None;
        self.crossed = Crossed::default();
    }

    pub fn wants_anything(&self) -> bool {
        self.lease.is_some()
    }

    /// Whether the caller has to read a status for the agent document. A
    /// telemetry only client never does, which is what keeps a ten per second
    /// tick off the mixer's command queue.
    pub fn needs_status(&self) -> bool {
        self.agent_wanted
    }

    /// Wait until there is something to send. Cancel safe, so it sits in a
    /// `select!` arm beside the socket and the event stream.
    pub async fn due(&self) {
        let Some(at) = self.next_due() else {
            return std::future::pending().await;
        };
        tokio::time::sleep_until(at).await
    }

    fn next_due(&self) -> Option<TokioInstant> {
        if !self.wants_anything() {
            return None;
        }
        // The agent push is checked at the telemetry rate, or once a second
        // when only thresholds were asked for.
        let hz = if self.hz > 0 { self.hz } else { 1 };
        let period = Duration::from_millis(1_000 / hz.clamp(1, 10) as u64);
        let last = self.last_tick?;
        Some(TokioInstant::from_std(last + period))
    }

    /// The messages to send now: at most one telemetry tick and one agent
    /// state, in that order.
    ///
    /// `live` is source id to liveness, which the connection already keeps so
    /// that nothing here has to ask the mixer for a status ten times a second.
    pub fn messages(
        &mut self,
        live: BTreeMap<String, u8>,
        program: Option<&str>,
        agent_document: impl FnOnce() -> Value,
    ) -> Vec<(&'static str, Value)> {
        let mut out = Vec::new();
        if !self.wants_anything() {
            return out;
        }
        self.last_tick = Some(Instant::now());
        let reading = telemetry::telemetry().read();
        let thresholds = self.thresholds.unwrap_or_default();
        if self.hz > 0 {
            let tick = reading.tick(live, thresholds.freeze_ms, thresholds.silence_ms);
            out.push(("telemetry", serde_json::to_value(tick).unwrap_or(Value::Null)));
        }
        if self.thresholds.is_none() {
            return out;
        }
        let now = Crossed {
            shot: reading.shot >= thresholds.shot,
            black: reading.black >= thresholds.black,
            freeze: reading.freeze(thresholds.freeze_ms),
            silence: reading.silence(thresholds.silence_ms),
        };
        let flipped = program.map(str::to_string) != self.program;
        let why = trigger(self.crossed, now, flipped);
        self.crossed = now;
        self.program = program.map(str::to_string);
        let Some(why) = why else { return out };
        if self.last_agent.is_some_and(|at| at.elapsed() < AGENT_MIN_GAP) {
            return out;
        }
        self.last_agent = Some(Instant::now());
        let mut document = agent_document();
        if let Some(map) = document.as_object_mut() {
            map.insert("why".into(), json!(why));
            // The one thing 09 section 5 item 12 asks for beside the numbers:
            // where to look if the numbers are not enough.
            map.entry("snapshot")
                .or_insert_with(|| json!(crate::control::methods::agent::SNAPSHOT_URL));
        }
        out.push(("agent.state", document));
        out
    }
}

/// Which condition turned true since the last push, if any. Edge triggered:
/// a picture that stays black is one message, not one a second.
fn trigger(before: Crossed, now: Crossed, program_flipped: bool) -> Option<&'static str> {
    if program_flipped {
        return Some("program");
    }
    if now.black && !before.black {
        return Some("black");
    }
    if now.freeze && !before.freeze {
        return Some("freeze");
    }
    if now.silence && !before.silence {
        return Some("silence");
    }
    if now.shot && !before.shot {
        return Some("shot");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_protocol::requests::TelemetryExt;

    fn ext(telemetry: Option<TelemetryExt>, agent: Option<AgentExt>) -> Ext {
        Ext { telemetry, agent, ..Ext::default() }
    }

    #[test]
    fn nothing_runs_until_a_client_asks_and_stops_when_it_stops_asking() {
        let mut push = Push::none();
        assert!(!push.wants_anything());

        push.configure(&ext(Some(TelemetryExt::On { hz: Some(4) }), None));
        assert!(push.wants_anything());
        assert_eq!(push.hz, 4);

        push.configure(&ext(None, None));
        assert!(!push.wants_anything(), "re-subscribing without it lets the probes go");
    }

    #[test]
    fn the_thresholds_have_the_defaults_the_plan_names() {
        let d = Thresholds::from(&AgentExt::On(true));
        assert_eq!(d.shot, 0.3);
        assert_eq!(d.black, 0.98);
        assert_eq!(d.freeze_ms, 200);
        assert_eq!(d.silence_ms, 500);

        let mine = Thresholds::from(&AgentExt::Thresholds {
            shot: Some(0.5),
            black: None,
            freeze_ms: Some(1_000),
            silence_ms: None,
        });
        assert_eq!(mine.shot, 0.5);
        assert_eq!(mine.black, 0.98, "what is not named keeps its default");
        assert_eq!(mine.freeze_ms, 1_000);
    }

    /// Edge triggered: a picture that stays black is one message, not one a
    /// tick, and a take is always worth telling an agent about.
    #[test]
    fn a_push_happens_on_the_edge_and_on_a_take() {
        let off = Crossed::default();
        let black = Crossed { black: true, ..off };
        assert_eq!(trigger(off, black, false), Some("black"));
        assert_eq!(trigger(black, black, false), None, "still black is not news");
        assert_eq!(trigger(black, off, false), None, "coming back is not a threshold crossing");
        assert_eq!(trigger(black, black, true), Some("program"), "a take always is");
        assert_eq!(
            trigger(off, Crossed { freeze: true, ..off }, false),
            Some("freeze")
        );
        assert_eq!(trigger(off, Crossed { shot: true, ..off }, false), Some("shot"));
    }

    #[test]
    fn a_telemetry_tick_carries_the_liveness_the_connection_already_knows() {
        let mut push = Push::none();
        push.configure(&ext(Some(TelemetryExt::On { hz: Some(2) }), None));
        let live = BTreeMap::from([("cam1".to_string(), 1u8), ("cam2".to_string(), 0)]);
        let messages = push.messages(live, Some("cam1"), || json!({}));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "telemetry");
        assert_eq!(messages[0].1["sources"]["cam1"], 1);
        assert_eq!(messages[0].1["sources"]["cam2"], 0);
        assert!(messages[0].1["ts"].as_u64().unwrap() > 0);
    }

    /// The first look at the world is not a threshold crossing, so a client
    /// that has just subscribed is not immediately sent a document; a take
    /// after that is.
    #[test]
    fn the_agent_push_waits_for_something_to_happen() {
        let mut push = Push::none();
        push.configure(&ext(None, Some(AgentExt::On(true))));
        let none = BTreeMap::new();
        let first = push.messages(none.clone(), None, || json!({ "program": null }));
        assert!(first.is_empty(), "{first:?}");

        let after = push.messages(none, Some("cam1"), || json!({ "program": "cam1" }));
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].0, "agent.state");
        assert_eq!(after[0].1["why"], "program");
        assert_eq!(after[0].1["snapshot"], crate::control::methods::agent::SNAPSHOT_URL);
    }
}
