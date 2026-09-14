//! The loop: read the state, decide, take.
//!
//! It polls `agent.state` rather than subscribing, which is what
//! `docs/agents.md` recommends and what costs the core least: a director works
//! on its own tempo and nothing it does needs to be woken by an event. The
//! document is a few hundred bytes and carries the motion score, which is the
//! one number the rules cannot work out for themselves.

use std::sync::Arc;
use std::time::Instant;

use godwinmix_client::{Client, Error};
use serde_json::{json, Value};
use tokio::sync::watch;

use crate::llm;
use crate::rules::{self, Decision, Shot, View};
use crate::settings::Settings;

/// Where log lines go: the core's log in a sidecar, stderr by hand.
pub trait Log: Send + Sync + 'static {
    fn info(&self, message: &str);
    fn warn(&self, message: &str);
}

pub struct Stderr;

impl Log for Stderr {
    fn info(&self, message: &str) {
        eprintln!("gmx-director: {message}");
    }
    fn warn(&self, message: &str) {
        eprintln!("gmx-director: {message}");
    }
}

pub struct Wiring {
    pub url: String,
    pub token: Option<String>,
    pub log: Arc<dyn Log>,
}

/// Turn the `agent.state` document into the view the rules read.
///
/// Anything missing is at its ordinary value, which is how the document is
/// written: a camera that is live, has a picture and has sound is four fields.
pub fn view_from(document: &Value, held_secs: f64) -> View {
    let sources = document
        .get("sources")
        .and_then(Value::as_array)
        .map(|list| list.iter().map(shot_from).collect())
        .unwrap_or_default();
    View {
        program: document
            .get("program")
            .and_then(Value::as_str)
            .map(str::to_string),
        sources,
        held_secs,
        held_by_core: document
            .get("held")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn shot_from(value: &Value) -> Shot {
    Shot {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        state: value
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("connecting")
            .to_string(),
        motion: value.get("motion").and_then(Value::as_f64),
        video_idle_ms: value.get("video_idle_ms").and_then(Value::as_u64),
        no_video: value
            .get("no_video")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        no_audio: value
            .get("no_audio")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// What the director remembers between cycles. Nothing else: the mixer is the
/// state, and a director that keeps its own copy is a director that argues
/// with reality.
pub struct Memory {
    /// When the last take landed, so the hold can be measured. Starts far
    /// enough back that the first cycle is free to act.
    pub last_take: Instant,
    pub last_program: Option<String>,
}

impl Default for Memory {
    fn default() -> Memory {
        Memory {
            last_take: Instant::now() - std::time::Duration::from_secs(3_600),
            last_program: None,
        }
    }
}

impl Memory {
    /// Seconds the current shot has been held. A take somebody else made,
    /// through the UI or over OSC, resets it too: the hold is about what the
    /// audience sees, not about who asked for it.
    pub fn held_secs(&mut self, program: Option<&str>) -> f64 {
        if self.last_program.as_deref() != program {
            self.last_program = program.map(str::to_string);
            self.last_take = Instant::now();
        }
        self.last_take.elapsed().as_secs_f64()
    }
}

/// Run until the process is told to stop.
pub async fn run(wiring: Wiring, mut settings: watch::Receiver<Settings>) {
    let mut memory = Memory::default();
    loop {
        let client = match Client::connect(&wiring.url, wiring.token.as_deref()).await {
            Ok(client) => client,
            Err(error) => {
                wiring.log.warn(&format!(
                    "cannot reach the core at {}: {error}. Trying again in two seconds.",
                    wiring.url
                ));
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };
        let current = settings.borrow().clone();
        wiring.log.info(&format!(
            "directing {} {} ({:.0}s hold{})",
            wiring.url,
            if current.uses_a_model() {
                format!("with '{}'", current.llm)
            } else {
                "on the rules".into()
            },
            current.min_hold_secs,
            if current.dry_run { ", dry run" } else { "" }
        ));

        let reason = serve(&wiring, &client, &mut settings, &mut memory).await;
        client.close();
        wiring.log.info(&format!("{reason}; reconnecting"));
    }
}

async fn serve(
    wiring: &Wiring,
    client: &Client,
    settings: &mut watch::Receiver<Settings>,
    memory: &mut Memory,
) -> String {
    loop {
        let current = settings.borrow().clone();
        let started = Instant::now();
        match cycle(wiring, client, &current, memory).await {
            Ok(()) => {}
            Err(Error::Closed) => return "the core closed the connection".into(),
            Err(error) => wiring.log.warn(&format!("state: {error}")),
        }
        let interval = std::time::Duration::from_secs_f64(current.interval_secs);
        let left = interval.saturating_sub(started.elapsed());
        tokio::select! {
            _ = tokio::time::sleep(left) => {}
            _ = settings.changed() => {
                wiring.log.info("the settings changed");
            }
        }
    }
}

/// One decision.
async fn cycle(
    wiring: &Wiring,
    client: &Client,
    settings: &Settings,
    memory: &mut Memory,
) -> Result<(), Error> {
    let document: Value = client.call_value("agent.state", json!({})).await?;
    let program = document.get("program").and_then(Value::as_str);
    let held = memory.held_secs(program);
    let view = view_from(&document, held);

    let decision = if settings.uses_a_model() {
        decide_with_model(wiring, settings, &view).await
    } else {
        rules::decide(&view, settings)
    };

    match decision {
        Decision::Hold(why) => {
            wiring.log.info(&format!("hold: {why}"));
            Ok(())
        }
        Decision::Take { source, why } if settings.dry_run => {
            wiring.log.info(&format!(
                "would take {}: {why}",
                source.as_deref().unwrap_or("the slate")
            ));
            Ok(())
        }
        Decision::Take { source, why } => {
            let params = match &source {
                Some(id) => json!({"source": id}),
                None => json!({"source": null}),
            };
            match client.call_value("program.take", params).await {
                Ok(_) => {
                    memory.last_take = Instant::now();
                    memory.last_program = source.clone();
                    wiring.log.info(&format!(
                        "take {}: {why}",
                        source.as_deref().unwrap_or("the slate")
                    ));
                    Ok(())
                }
                // A refused take is the core doing its job. Log it and leave
                // the hold where it was, so the next cycle tries again.
                Err(Error::Closed) => Err(Error::Closed),
                Err(error) => {
                    wiring.log.warn(&format!(
                        "take {} refused: {error}",
                        source.as_deref().unwrap_or("the slate")
                    ));
                    Ok(())
                }
            }
        }
    }
}

/// Ask the model, and put its answer through the same rules.
async fn decide_with_model(wiring: &Wiring, settings: &Settings, view: &View) -> Decision {
    let prompt = llm::prompt(view, settings);
    match llm::consult(settings, &prompt).await {
        Ok(proposal) => rules::check(view, settings, proposal.take.as_deref(), &proposal.reason),
        Err(trouble) => {
            // Falling back is the whole point of having rules underneath: a
            // model that is slow, absent or confused costs this cycle its
            // opinion and nothing else.
            wiring
                .log
                .warn(&format!("{trouble}; deciding on the rules for this cycle"));
            rules::decide(view, settings)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_document_becomes_a_view() {
        let document = json!({
            "program": "cam1",
            "program_motion": 0.31,
            "uptime_secs": 12,
            "sources": [
                {"id": "cam1", "state": "live", "motion": 0.31},
                {"id": "score", "state": "live", "no_audio": true, "motion": 0.0},
                {"id": "guest", "state": "connecting", "no_video": true, "no_audio": true}
            ]
        });
        let view = view_from(&document, 4.0);
        assert_eq!(view.program.as_deref(), Some("cam1"));
        assert_eq!(view.held_secs, 4.0);
        assert_eq!(view.sources.len(), 3);
        assert_eq!(view.sources[1].motion, Some(0.0));
        assert!(view.sources[1].no_audio);
        assert!(view.sources[2].no_video);
        assert_eq!(view.sources[2].state, "connecting");
        assert!(view.held_by_core.is_none());
    }

    #[test]
    fn a_slate_and_an_empty_mixer_read_cleanly() {
        let view = view_from(&json!({"program": null, "uptime_secs": 0}), 0.0);
        assert!(view.program.is_none());
        assert!(view.sources.is_empty());
    }

    #[test]
    fn a_core_that_is_holding_the_programme_says_so_in_the_view() {
        let view = view_from(&json!({"program": "cam1", "held": "the ad break is on air"}), 1.0);
        assert_eq!(view.held_by_core.as_deref(), Some("the ad break is on air"));
    }

    #[test]
    fn a_source_with_no_state_is_read_as_connecting_rather_than_live() {
        let view = view_from(&json!({"sources": [{"id": "cam1"}]}), 0.0);
        assert_eq!(view.sources[0].state, "connecting");
        assert!(!view.sources[0].is_live(), "an unknown state is never taken");
    }

    #[test]
    fn the_hold_restarts_when_the_programme_changes_under_us() {
        let mut memory = Memory::default();
        let first = memory.held_secs(Some("cam1"));
        assert!(first < 0.1, "a new programme starts the hold at zero, got {first}");
        let again = memory.held_secs(Some("cam1"));
        assert!(again >= first, "the same programme keeps counting");
        let moved = memory.held_secs(Some("cam2"));
        assert!(moved < 0.1, "somebody else took cam2, so the hold restarts");
    }

    #[test]
    fn the_first_cycle_is_free_to_act() {
        let mut memory = Memory {
            last_program: None,
            ..Memory::default()
        };
        assert!(memory.held_secs(None) > 3_000.0, "nothing has been on air yet");
    }
}
