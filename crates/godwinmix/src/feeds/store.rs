//! Feeds and bindings on disk, beside the show's config, and header values
//! in the secret store.
//!
//! `godwinmix.toml` keeps its feeds in `godwinmix.feeds.json`, so a show
//! copied with `show.add` takes its feeds with it (every file named for the
//! config's stem goes) and a restart brings them back. The file holds header
//! names with `"__secret__"` for each value; the values are sealed in the
//! secret store under `feed.<show>.<feed>`, the way a plugin's secret
//! settings are.

use super::{Binding, Feed, Feeds, State};
use godwinmix_protocol::feeds::{BindingSpec, FeedSpec, SECRET_SENTINEL};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    feeds: Vec<FeedSpec>,
    #[serde(default)]
    bindings: Vec<BindingSpec>,
}

pub fn path_beside(config: &Path) -> PathBuf {
    let stem = config.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "godwinmix".into());
    config.with_file_name(format!("{stem}.feeds.json"))
}

fn secrets() -> &'static godwinmix_core::secrets::Secrets {
    crate::control::methods::plugins::secrets()
}

impl Feeds {
    fn secret_scope(&self, feed: &str) -> String {
        format!("{}.{feed}", self.scope)
    }

    /// Seal every header value that is not the sentinel; give the sentinel
    /// back the value stored for it. Answers the headers with real values.
    pub(super) fn seal(&self, spec: &mut FeedSpec, before: Option<&FeedSpec>) {
        let scope = self.secret_scope(&spec.id);
        for (name, value) in spec.headers.iter_mut() {
            if value == SECRET_SENTINEL {
                let kept = before.and_then(|b| b.headers.get(name)).cloned();
                *value = kept.or_else(|| secrets().get(&scope, name)).unwrap_or_default();
            } else if self.file.is_some() {
                if let Err(e) = secrets().set(&scope, name, value) {
                    tracing::warn!(feed = %spec.id, header = %name, error = %format!("{e:#}"), "a feed header could not be sealed; it lasts until a restart");
                }
            }
        }
    }

    pub(super) fn forget_secrets(&self, feed: &str) {
        secrets().forget(&self.secret_scope(feed));
    }

    pub(super) fn load(&self) {
        let Some(path) = &self.file else { return };
        let file: File = match std::fs::read_to_string(path) {
            Err(_) => return,
            Ok(text) => match serde_json::from_str(&text) {
                Ok(f) => f,
                Err(e) => {
                    let aside = path.with_extension("json.unreadable");
                    let _ = std::fs::rename(path, &aside);
                    tracing::error!(file = %path.display(), moved_to = %aside.display(), error = %e, "the feeds file would not parse; it was moved aside and this show starts with no feeds");
                    return;
                }
            },
        };
        let mut st = self.state.lock();
        for mut spec in file.feeds {
            self.seal(&mut spec, None);
            st.feeds.insert(spec.id.clone(), Feed::new(spec));
        }
        for spec in file.bindings {
            st.bindings.insert(spec.id.clone(), Binding::new(spec));
        }
    }

    /// Write the file, atomically. A core with no config file keeps its
    /// feeds in memory only.
    pub(super) fn save(&self, st: &State) {
        let Some(path) = &self.file else { return };
        let file = File {
            feeds: st.feeds.values().map(|f| hidden(&f.spec)).collect(),
            bindings: st.bindings.values().map(|b| b.spec.clone()).collect(),
        };
        let text = serde_json::to_string_pretty(&file).unwrap_or_default();
        let tmp = path.with_extension("json.tmp");
        let written = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path));
        if let Err(e) = written {
            tracing::error!(file = %path.display(), error = %e, "the feeds could not be saved; they last until a restart");
        }
    }
}

/// The spec with every header value replaced by the sentinel.
pub fn hidden(spec: &FeedSpec) -> FeedSpec {
    let mut out = spec.clone();
    for v in out.headers.values_mut() {
        *v = SECRET_SENTINEL.into();
    }
    out
}

impl Feed {
    pub(super) fn new(spec: FeedSpec) -> Feed {
        Feed { spec, run: Default::default(), doc: None, task: None, wake: Default::default() }
    }
}

impl Binding {
    pub(super) fn new(spec: BindingSpec) -> Binding {
        Binding { spec, written: None, last_write: None, writes: 0, last_error: None, failures: 0 }
    }
}
