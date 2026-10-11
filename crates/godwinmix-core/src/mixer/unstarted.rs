//! Sources the mixer was asked for and could not start.
//!
//! A configured source that fails at boot (a web page with no browser beside
//! the mixer, a file not uploaded yet, a camera that is not plugged in) used
//! to be logged and forgotten. The scenes that draw it still named it, and a
//! page could say only that it was "not running", never why, and had no way
//! to put it back. Worse, the next write of the runtime source list left it
//! out, so after one restart it was gone from the list for good.
//!
//! So the mixer keeps each one here with the error it failed with and the
//! button that fixes it, when the error carries one. A source that starts
//! later, however it gets there, leaves the list.
//!
//! And it is tried again, unless it waits on a piece being set up, which
//! starts it when the piece is ready. On 2026-10-05 a USB webcam answered
//! `start` after more than five seconds at every boot of the desktop app.
//! Each time the core gave up on it, kept it here, and never asked again, so
//! the camera was missing from every show until somebody added it by hand.
//! The wait between tries is `backoff::unstarted_delay`.

use super::{backoff, Command, Mixer};
use crate::config::SourceConfig;
use crate::state::{Event, SourceId, SourceState};
use godwinmix_protocol::ErrorAction;
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, info};

/// One source that could not be started, as it was asked for.
#[derive(Debug, Clone)]
pub struct Unstarted {
    pub config: SourceConfig,
    /// The error it failed with, which names the next step.
    pub error: String,
    /// What fixes it, when the error knows: install a plugin, set a key.
    pub action: Option<ErrorAction>,
}

/// The list, one entry an id, the newest failure winning, and for each id
/// that is being tried again how many tries have failed since the first and
/// when that was.
///
/// The count is kept here rather than in the rebuild count, because every
/// failed try is added and removed again, and the removal clears everything
/// the mixer keeps under the id: the wait between tries stayed at half a
/// second for good.
#[derive(Debug, Clone, Default)]
pub struct UnstartedList(Vec<Unstarted>, HashMap<String, (u32, Instant)>);

impl UnstartedList {
    /// One more failed try of `id`: how many failed before this one.
    pub fn tried(&mut self, id: &str) -> u32 {
        let (count, _) = self.1.entry(id.to_string()).or_insert((0, Instant::now()));
        *count += 1;
        *count - 1
    }

    /// When `id` was first tried, from its first failure until it has been
    /// live.
    pub fn first_tried(&self, id: &str) -> Option<Instant> {
        self.1.get(id).map(|t| t.1)
    }

    /// How many tries of `id` have failed since it was last live.
    pub fn failures(&self, id: &str) -> u32 {
        self.1.get(id).map_or(0, |t| t.0)
    }

    /// It has been live, or was taken away: the next failure starts over.
    pub fn started(&mut self, id: &str) {
        self.1.remove(id);
    }

    /// Remember a failure, replacing any earlier one for the same id.
    pub fn note(&mut self, config: &SourceConfig, error: &anyhow::Error) {
        self.forget(&config.id);
        self.0.push(Unstarted {
            config: config.clone(),
            error: crate::setup::plain::for_person(error).0,
            action: ErrorAction::find(error.as_ref()),
        });
    }

    /// The source started, or was taken away on purpose.
    pub fn forget(&mut self, id: &str) {
        self.0.retain(|u| u.config.id != id);
    }

    pub fn entries(&self) -> &[Unstarted] {
        &self.0
    }

    /// The configs, for the runtime list, so a source that failed this time
    /// is still asked for next time.
    pub fn configs(&self) -> impl Iterator<Item = &SourceConfig> {
        self.0.iter().map(|u| &u.config)
    }
}

impl Mixer {
    /// Whether `id` can go on program. A source still waiting to start can,
    /// when it has a backup: the backup is drawn until it starts, which is
    /// what an operator who set a backup expects of a camera not there yet.
    pub(super) fn takeable(&self, id: &SourceId) -> anyhow::Result<()> {
        if self.sources.iter().any(|s| &s.input.id == id) {
            return Ok(());
        }
        let Some(waiting) = self.unstarted.entries().iter().find(|u| u.config.id == *id) else {
            anyhow::bail!("no such source {id}");
        };
        if self.backups.rule(id).is_some() {
            return Ok(());
        }
        anyhow::bail!(
            "{id} has not started ({}) and is tried again by itself; take it once it is live, or give it a backup to show meanwhile",
            waiting.error
        )
    }

    /// Ask for an unstarted source again after the backoff, counting the
    /// failure. The supervisor clears the count once the source has been
    /// live, so a source that came up and later failed again starts from the
    /// short waits.
    pub(super) fn retry_unstarted_later(&mut self, id: &SourceId) {
        // It was said to be connecting before the build began, and it is
        // not: a page following the stream should not wait on it.
        let _ = self.events.send(Event::SourceStateChanged {
            source: id.clone(),
            state: SourceState::Failed,
        });
        let before = self.unstarted.tried(id);
        // One with a backup is a camera the operator expects back and is
        // covering meanwhile, so it keeps the restart curve, up to ten
        // seconds, rather than moving to tries minutes apart.
        let delay = match self.backups.rule(id) {
            Some(_) => backoff::restart_delay(before),
            None => backoff::unstarted_delay(&self.cfg.stall, before),
        };
        info!(source = %id, failures = before + 1, ?delay, "the source did not start; trying it again later");
        let handle = self.handle.clone();
        let again = id.clone();
        self.rt.spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = handle.send(Command::RetryUnstarted(again));
        });
    }

    /// The backoff has run out: build it again, unless it has started, been
    /// removed or been asked for again by somebody meanwhile.
    pub(super) fn retry_unstarted(&mut self, id: &SourceId) {
        let busy = self.sources.iter().any(|s| &s.input.id == id)
            || self.pending.iter().any(|c| &c.id == id)
            || self.rebuilding.contains_key(id);
        let Some(cfg) = self.unstarted.configs().find(|c| &c.id == id).cloned() else {
            return;
        };
        if busy {
            return;
        }
        info!(source = %id, "trying a source that did not start again");
        // A failure notes it again and arms the next try; see `note_unstarted`.
        if let Err(e) = self.begin_add_source(cfg, None) {
            debug!(source = %id, ?e, "the source still did not start");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str) -> SourceConfig {
        toml::from_str(&format!("id = \"{id}\"\nuri = \"media/{id}.mp4\"\n")).expect("a source")
    }

    #[test]
    fn a_failure_is_kept_once_with_its_error_and_forgotten_when_it_starts() {
        let mut list = UnstartedList::default();
        list.note(&source("slides"), &anyhow::anyhow!("no file yet"));
        list.note(&source("slides"), &anyhow::anyhow!("still no file"));
        assert_eq!(list.entries().len(), 1);
        assert_eq!(list.entries()[0].error, "still no file");
        assert_eq!(list.configs().count(), 1);
        list.forget("slides");
        assert!(list.entries().is_empty());
    }

    /// Each failed try is noted and forgotten again as the source is added
    /// and removed, and the count of tries outlives that, so the wait grows
    /// from half a second to ten as it does for a restart.
    #[test]
    fn the_wait_between_tries_grows_until_the_source_has_been_live() {
        let stall = crate::config::StallConfig::default();
        let mut list = UnstartedList::default();
        let mut waits = Vec::new();
        for _ in 0..6 {
            list.note(&source("cam"), &anyhow::anyhow!("Could not open resource for reading"));
            waits.push(backoff::unstarted_delay(&stall, list.tried("cam")));
            list.forget("cam");
        }
        assert_eq!(waits[0], std::time::Duration::from_millis(500));
        assert!(waits.windows(2).all(|w| w[1] >= w[0]) && waits[5] > waits[0], "{waits:?}");
        assert!(list.first_tried("cam").is_some());
        list.started("cam");
        assert_eq!((list.failures("cam"), list.first_tried("cam")), (0, None));
    }
}
