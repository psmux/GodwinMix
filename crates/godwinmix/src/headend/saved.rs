//! Which show each Flussonic stream runs as, and the plan it was made from,
//! kept beside the list of shows (`flussonic-shows.json`). With it a
//! restarted headend finds its shows already running and restarts none of
//! them unless the file changed while it was down.

use crate::station::state::Station;
use godwinmix_flussonic::import::ShowPlan;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const FILE: &str = "flussonic-shows.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Applied {
    /// The show's id, which the station made from the stream's name.
    pub id: String,
    pub plan: ShowPlan,
}

pub type Running = BTreeMap<String, Applied>;

fn path(st: &Station) -> PathBuf {
    st.registry.lock().data_dir().join(FILE)
}

/// What was kept, less any show that is no longer in the list.
pub fn load(st: &Station) -> Running {
    let Ok(text) = std::fs::read_to_string(path(st)) else { return Running::new() };
    let kept: Running = match serde_json::from_str(&text) {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!(error = %e, file = FILE, "the headend's list of shows would not read; every stream is made again");
            return Running::new();
        }
    };
    kept.into_iter().filter(|(_, a)| st.is_direct(&a.id)).collect()
}

pub fn save(st: &Station, running: &Running) {
    let p = path(st);
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(running).unwrap_or_default();
    if let Err(e) = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &p)) {
        tracing::warn!(error = %e, file = %p.display(), "could not keep the headend's list of shows");
    }
}
