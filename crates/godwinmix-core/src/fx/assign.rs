//! Which transition a take uses when it names none: one for every take, and
//! one per scene, kept in the library folder as `fx-assign.json` so it lasts
//! across restarts and every desk and agent sees the same choice.

use anyhow::{Context, Result};
use godwinmix_protocol::fx::FxAssignments;
use std::path::Path;

const FILE: &str = "fx-assign.json";

/// What is assigned. An unreadable file is nothing assigned, so a take is
/// never refused for it.
pub fn load(root: &Path) -> FxAssignments {
    std::fs::read_to_string(root.join(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Set or clear the transition for `scene`, or the default when `scene` is
/// `None`, and answer what is assigned now.
pub fn set(root: &Path, scene: Option<&str>, transition: Option<&str>) -> Result<FxAssignments> {
    let mut a = load(root);
    let name = transition.map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty());
    match (scene, name) {
        (None, n) => a.default = n,
        (Some(s), Some(n)) => {
            a.scenes.insert(s.to_string(), n);
        }
        (Some(s), None) => {
            a.scenes.remove(s);
        }
    }
    std::fs::create_dir_all(root).with_context(|| format!("making {}", root.display()))?;
    let part = root.join(".fx-assign.json.part");
    std::fs::write(&part, serde_json::to_string_pretty(&a)? + "\n").context("writing fx-assign.json")?;
    std::fs::rename(&part, root.join(FILE)).context("saving fx-assign.json")?;
    Ok(a)
}

/// The transition for a take of `scene`: the scene's own, then the default.
pub fn for_scene(root: &Path, scene: Option<&str>) -> Option<String> {
    let a = load(root);
    scene.and_then(|s| a.scenes.get(s).cloned()).or(a.default)
}
