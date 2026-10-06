//! Outputs the mixer was asked for at start and could not attach.
//!
//! A show started again by its station attaches the outputs it kept. On a
//! busy machine the governor can say no to an output's rendition just then
//! ("0.0 cores is free"), and until 2026-10-06 that was one line in the log
//! and nothing else: the output was gone from the list, from the status and,
//! at the next write of the runtime store, from the file too. On a macOS
//! runner the station later had 1.4 cores free and the output never came
//! back, because nobody asked for it again.
//!
//! So each one is kept here with what it failed with, reported as a failed
//! output whose `shed` says why, written to the runtime store as it was, and
//! tried again on the watchdog tick after a backoff. A governor refusal costs
//! nothing to ask again (no element is built before the governor says yes),
//! so it waits on the short restart curve, half a second to ten. Anything
//! else, a plugin that is missing or an address that does not parse, waits on
//! the source curve, which stretches to the rebuild backoff.

use super::{backoff, Mixer};
use crate::config::{OutputConfig, StallConfig};
use crate::state::{safe_uri_label, uri_has_key, OutputState, OutputStatus};
use godwinmix_protocol::error::ErrorCode;
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// One output that could not be attached, as it was asked for.
#[derive(Debug, Clone)]
pub struct Unattached {
    pub config: OutputConfig,
    /// The error it failed with last, which names the next step.
    pub error: String,
    failures: u32,
    next: Instant,
}

/// The list, one entry an id.
#[derive(Debug, Clone, Default)]
pub struct UnattachedList(Vec<Unattached>);

impl UnattachedList {
    /// Remember a failure, counting it against any earlier one for the id.
    pub fn note(&mut self, config: &OutputConfig, error: &anyhow::Error, stall: &StallConfig, now: Instant) {
        let failures = self.0.iter().find(|u| u.config.id == config.id).map_or(0, |u| u.failures);
        self.forget(&config.id);
        let wait = delay(stall, failures, by_governor(error));
        self.0.push(Unattached {
            config: config.clone(),
            error: crate::setup::plain::for_person(error).0,
            failures: failures + 1,
            next: now + wait,
        });
    }

    /// It attached, or was taken away on purpose. True when it was here.
    pub fn forget(&mut self, id: &str) -> bool {
        let before = self.0.len();
        self.0.retain(|u| u.config.id != id);
        self.0.len() != before
    }

    pub fn has(&self, id: &str) -> bool {
        self.0.iter().any(|u| u.config.id == id)
    }

    pub fn configs(&self) -> impl Iterator<Item = &OutputConfig> {
        self.0.iter().map(|u| &u.config)
    }

    /// The ones whose wait has run out.
    pub fn due(&self, now: Instant) -> Vec<OutputConfig> {
        self.0.iter().filter(|u| u.next <= now).map(|u| u.config.clone()).collect()
    }

    /// Each as a failed output with the reason in `shed`, for the status.
    pub fn statuses(&self) -> impl Iterator<Item = OutputStatus> + '_ {
        self.0.iter().map(|u| OutputStatus {
            id: u.config.id.clone(),
            uri_host: safe_uri_label(&u.config.uri),
            has_key: uri_has_key(&u.config.uri),
            state: OutputState::Failed,
            reconnects: 0,
            queue_secs: 0.0,
            rendition: u.config.rendition.clone(),
            shed: Some(format!("Not attached yet, and tried again by itself: {}", u.error)),
            extra: Default::default(),
        })
    }
}

/// Whether the governor said no, rather than the output itself failing.
fn by_governor(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|c| c.downcast_ref::<crate::render::Refusal>())
        .any(|r| r.code == ErrorCode::Safety)
}

/// The wait before try number `failures + 1`.
fn delay(stall: &StallConfig, failures: u32, governor: bool) -> Duration {
    if governor {
        backoff::restart_delay(failures)
    } else {
        backoff::unstarted_delay(stall, failures)
    }
}

impl Mixer {
    /// An output from the config or the runtime store that would not attach
    /// at start: kept, and tried again from the tick.
    pub(super) fn keep_unattached(&mut self, cfg: &OutputConfig, error: &anyhow::Error) {
        self.unattached.note(cfg, error, &self.cfg.stall, Instant::now());
    }

    /// Once a watchdog tick: attach whatever has waited long enough.
    pub(super) fn retry_unattached(&mut self) {
        let due = self.unattached.due(Instant::now());
        if due.is_empty() {
            return;
        }
        for cfg in due {
            match self.attach_output(&cfg) {
                Ok(slot) => {
                    self.unattached.forget(&cfg.id);
                    self.outputs.push(slot);
                    info!(output = %cfg.id, "an output that would not attach at start is attached now");
                }
                Err(e) => {
                    debug!(output = %cfg.id, ?e, "the output still would not attach");
                    self.keep_unattached(&cfg, &e);
                }
            }
        }
        self.note_on_air();
        self.broadcast_status();
    }
}

#[cfg(test)]
mod tests;
