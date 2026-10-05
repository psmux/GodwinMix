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

/// The list, one entry an id, the newest failure winning.
#[derive(Debug, Clone, Default)]
pub struct UnstartedList(Vec<Unstarted>);

impl UnstartedList {
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
    /// Ask for an unstarted source again after the backoff, counting the
    /// failure. The count is the rebuild one, which the supervisor clears
    /// once the source has been live, so a source that came up and later
    /// failed again starts from the short waits.
    pub(super) fn retry_unstarted_later(&mut self, id: &SourceId) {
        // It was said to be connecting before the build began, and it is
        // not: a page following the stream should not wait on it.
        let _ = self.events.send(Event::SourceStateChanged {
            source: id.clone(),
            state: SourceState::Failed,
        });
        let failures = self.rebuild_failures.entry(id.clone()).or_insert(0);
        let delay = backoff::unstarted_delay(&self.cfg.stall, *failures);
        *failures += 1;
        info!(source = %id, failures = *failures, ?delay, "the source did not start; trying it again later");
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
}
