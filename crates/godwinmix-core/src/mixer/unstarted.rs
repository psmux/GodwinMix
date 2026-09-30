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

use crate::config::SourceConfig;
use godwinmix_protocol::ErrorAction;

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
            error: format!("{error:#}"),
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
