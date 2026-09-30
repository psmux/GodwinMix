//! Where a calibration is kept, and when to take a new one.
//!
//! One small JSON file per machine under `<data dir>/governor/`, named by
//! the fingerprint, so a data directory copied to another machine (or the
//! same machine after a new GPU) is measured again rather than trusted.

use crate::calibration::{Calibration, FORMAT};
use std::path::{Path, PathBuf};

pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// `data_dir` is the mixer's runtime directory (`.godwinmix` beside the
    /// config, or `GODWINMIX_RUNTIME_DIR`).
    pub fn new(data_dir: &Path) -> Store {
        Store { dir: data_dir.join("governor") }
    }

    pub fn path(&self, fingerprint: &str) -> PathBuf {
        self.dir.join(format!("calibration-{fingerprint}.json"))
    }

    /// The calibration for this fingerprint, if one was taken and still
    /// reads in this version's format.
    pub fn load(&self, fingerprint: &str) -> Option<Calibration> {
        read(&self.path(fingerprint)).filter(|c| c.fingerprint == fingerprint)
    }

    /// The newest calibration of any fingerprint: a stand in while the
    /// machine is on air and cannot be measured again yet.
    pub fn latest(&self) -> Option<Calibration> {
        let entries = std::fs::read_dir(&self.dir).ok()?;
        entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("calibration-"))
            .filter_map(|e| read(&e.path()))
            .max_by_key(|c| c.taken_unix)
    }

    /// Write it, whole or not at all: a temporary file renamed into place.
    pub fn save(&self, cal: &Calibration) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path(&cal.fingerprint);
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(cal).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }
}

fn read(path: &Path) -> Option<Calibration> {
    let text = std::fs::read_to_string(path).ok()?;
    let cal: Calibration = serde_json::from_str(&text).ok()?;
    (cal.format == FORMAT).then_some(cal)
}

/// What to do on start, or when asked.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// This machine was measured; use that.
    Use(Calibration),
    /// Measure now.
    Measure,
    /// Not now, something is on air. Use the stand in (possibly nothing)
    /// and measure once the air is clear.
    Wait { stand_in: Option<Calibration> },
}

/// Never measure while on air unless a person asked: the encodes take every
/// core for a few seconds, and the programme comes first.
pub fn decide(store: &Store, fingerprint: &str, on_air: bool, asked: bool) -> Decision {
    if asked {
        return Decision::Measure;
    }
    if let Some(c) = store.load(fingerprint) {
        return Decision::Use(c);
    }
    if on_air {
        return Decision::Wait { stand_in: store.latest() };
    }
    Decision::Measure
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gmx-govern-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn cal(fp: &str, at: u64) -> Calibration {
        Calibration { format: FORMAT, fingerprint: fp.into(), taken_unix: at, ..Default::default() }
    }

    #[test]
    fn a_saved_calibration_is_used_on_the_next_start() {
        let s = Store::new(&scratch("reuse"));
        assert_eq!(decide(&s, "aa", false, false), Decision::Measure);
        s.save(&cal("aa", 1)).unwrap();
        assert_eq!(decide(&s, "aa", false, false), Decision::Use(cal("aa", 1)));
    }

    #[test]
    fn a_new_fingerprint_measures_again_but_not_on_air() {
        let s = Store::new(&scratch("changed"));
        s.save(&cal("old", 5)).unwrap();
        assert_eq!(decide(&s, "new", false, false), Decision::Measure);
        assert_eq!(decide(&s, "new", true, false), Decision::Wait { stand_in: Some(cal("old", 5)) });
    }

    #[test]
    fn asking_measures_even_on_air() {
        let s = Store::new(&scratch("asked"));
        s.save(&cal("aa", 1)).unwrap();
        assert_eq!(decide(&s, "aa", true, true), Decision::Measure);
    }

    #[test]
    fn a_file_in_an_older_format_is_not_trusted() {
        let s = Store::new(&scratch("format"));
        let mut c = cal("aa", 1);
        c.format = FORMAT + 1;
        s.save(&c).unwrap();
        assert_eq!(s.load("aa"), None);
    }
}
