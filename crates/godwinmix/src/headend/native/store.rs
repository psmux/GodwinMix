//! The station's own headend config, `headend.json` beside the list of
//! shows: the channels, the profiles saved here, and which streams of a
//! followed Flussonic file were taken over by an edit on the headend
//! screen. Written whole, through a temporary file, on every change.

use godwinmix_protocol::headend::{HeadendChannel, TranscodeProfile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE: &str = "headend.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub channels: Vec<HeadendChannel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<TranscodeProfile>,
    /// A stream of the followed file, by name, that the station holds now,
    /// and the file it came from. With no channel of that name, the stream
    /// was removed on the headend screen.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub detached: BTreeMap<String, String>,
}

impl Store {
    pub fn channel(&self, name: &str) -> Option<&HeadendChannel> {
        self.channels.iter().find(|c| c.name == name)
    }

    pub fn put(&mut self, c: HeadendChannel) {
        match self.channels.iter_mut().find(|x| x.name == c.name) {
            Some(x) => *x = c,
            None => self.channels.push(c),
        }
    }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// What was kept, or nothing. A file that will not read is kept aside as
/// `headend.json.unread`, so a hand edit gone wrong loses nothing.
pub fn load(dir: &Path) -> Store {
    let p = path(dir);
    let Ok(text) = std::fs::read_to_string(&p) else { return Store::default() };
    match serde_json::from_str(&text) {
        Ok(s) => s,
        Err(e) => {
            let aside = p.with_extension("json.unread");
            let _ = std::fs::rename(&p, &aside);
            tracing::warn!(error = %e, file = %p.display(), aside = %aside.display(), "the headend's channels would not read; it starts with none, and the file is kept aside");
            Store::default()
        }
    }
}

pub fn save(dir: &Path, s: &Store) -> std::io::Result<()> {
    let p = path(dir);
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(s).unwrap_or_default())?;
    std::fs::rename(&tmp, &p)
}
