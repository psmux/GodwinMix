//! Which show each channel runs as, and the plan it was made from, kept
//! beside the list of shows: `flussonic-shows.json` for the streams of a
//! followed Flussonic file, `headend-shows.json` for the channels kept by
//! the station. With it a restarted headend finds its shows already running
//! and restarts none of them unless something changed while it was down.

use crate::station::state::Station;
use godwinmix_flussonic::import::ShowPlan;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The followed Flussonic file's streams.
pub const FLUSSONIC: &str = "flussonic-shows.json";
/// The channels kept by the station.
pub const NATIVE: &str = "headend-shows.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Applied {
    /// The show's id, which the station made from the stream's name.
    pub id: String,
    pub plan: ShowPlan,
}

pub type Running = BTreeMap<String, Applied>;

fn path(st: &Station, file: &str) -> PathBuf {
    st.registry.lock().data_dir().join(file)
}

/// What was kept, less any show that is no longer in the list.
pub fn load(st: &Station, file: &str) -> Running {
    let Ok(text) = std::fs::read_to_string(path(st, file)) else { return Running::new() };
    let kept: Running = match serde_json::from_str(&text) {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!(error = %e, file, "the headend's list of shows would not read; every stream is made again");
            return Running::new();
        }
    };
    kept.into_iter().filter(|(_, a)| st.is_direct(&a.id)).collect()
}

pub fn save(st: &Station, file: &str, running: &Running) {
    let p = path(st, file);
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(running).unwrap_or_default();
    if let Err(e) = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &p)) {
        tracing::warn!(error = %e, file = %p.display(), "could not keep the headend's list of shows");
    }
}
