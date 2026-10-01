//! Which shows a station has, and where each one's config is.
//!
//! The first show is the config the station was started with, run in place
//! as `main`: nothing is copied and its data stays where it is, so a single
//! show setup is exactly what it was. Every other show has a folder of its
//! own, `<data dir>/shows/<id>/`, with its config and everything a core keeps
//! beside its config. The list is `<data dir>/shows.json`, written only once
//! there is something a bare config cannot say (a second show, a renamed or
//! stopped first one).
//!
//! A show without compositing (wave 4) has no folder and no process: its
//! record carries its input and its outputs, and the direct host runs it.
//! A list written before wave 4 has neither field, and every show in it
//! composites, as it did.

mod output;
mod record;

pub use output::OutputRecord;
pub use record::Record;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The id the station's own config runs under.
pub const MAIN: &str = "main";

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    shows: Vec<Record>,
}

/// The list, and where it is kept.
#[derive(Debug)]
pub struct Registry {
    data_dir: PathBuf,
    main_config: PathBuf,
    pub records: Vec<Record>,
}

impl Registry {
    /// Read the list beside `main_config`, or make the one show list a bare
    /// config means. A list that will not parse is not written over.
    pub fn open(main_config: &Path) -> anyhow::Result<Registry> {
        let main_config = std::path::absolute(main_config)?;
        let data_dir = main_config.parent().unwrap_or(Path::new(".")).to_path_buf();
        let path = data_dir.join("shows.json");
        let records = match std::fs::read_to_string(&path) {
            Ok(text) => {
                let file: File = serde_json::from_str(&text).map_err(|e| {
                    anyhow::anyhow!("{} would not parse ({e}). Move it aside to start with one show.", path.display())
                })?;
                file.shows
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        let mut reg = Registry { data_dir, main_config, records };
        if !reg.records.iter().any(|r| r.id == MAIN) {
            reg.records.insert(0, Record::new(MAIN, "Main", None));
        }
        Ok(reg)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The config a show runs.
    pub fn config_of(&self, record: &Record) -> PathBuf {
        match &record.config {
            Some(p) if p.is_absolute() => p.clone(),
            Some(p) => self.data_dir.join(p),
            None => self.main_config.clone(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Record> {
        self.records.iter_mut().find(|r| r.id == id)
    }

    pub fn ids(&self) -> Vec<String> {
        self.records.iter().map(|r| r.id.clone()).collect()
    }

    /// The show a client reaches when it names none.
    pub fn first(&self) -> String {
        self.records.first().map(|r| r.id.clone()).unwrap_or_else(|| MAIN.into())
    }

    /// A free id made from a name: `Second room` is `second-room`, then
    /// `second-room-2`.
    pub fn free_id(&self, name: &str) -> String {
        let base = match crate::channels::keys::slug(name) {
            s if s.is_empty() => "show".to_string(),
            s => s,
        };
        std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base}-{n}")))
            .find(|id| self.get(id).is_none())
            .unwrap_or(base)
    }

    /// The folder a new show's files go in.
    pub fn folder_for(&self, id: &str) -> PathBuf {
        self.data_dir.join("shows").join(id)
    }

    /// Write the list, unless it says nothing a bare config does not.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = self.data_dir.join("shows.json");
        let bare = self.records.len() == 1 && self.records[0] == Record::new(MAIN, "Main", None);
        if bare && !path.exists() {
            return Ok(());
        }
        let text = serde_json::to_string_pretty(&File { shows: self.records.clone() })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
