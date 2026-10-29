//! The invites on disk, beside the show's config.
//!
//! `godwinmix.toml` keeps them in `godwinmix.guests.json`, so a restart
//! brings every invite back with the same link. The file holds each invite's
//! key, because the desk has to be able to show the link again, so it is
//! written owner readable only on macOS and Linux, through a temporary file
//! and a rename. A file that will not parse is moved aside rather than
//! written over.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One invite as it is kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub name: String,
    pub created: String,
    /// When it runs out, in milliseconds since the Unix epoch.
    pub expires_ms: u64,
    pub key: String,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    invites: Vec<Record>,
}

pub fn path_beside(config: &Path) -> PathBuf {
    let stem = config.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "godwinmix".into());
    config.with_file_name(format!("{stem}.guests.json"))
}

/// What was saved, less what has run out by `now_ms`.
pub fn load(path: &Path, now_ms: u64) -> Vec<Record> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    match serde_json::from_str::<File>(&text) {
        Ok(f) => f.invites.into_iter().filter(|r| r.expires_ms > now_ms).collect(),
        Err(e) => {
            let aside = path.with_extension("json.unreadable");
            let _ = std::fs::rename(path, &aside);
            tracing::error!(file = %path.display(), moved_to = %aside.display(), error = %e, "the guest invites file would not parse; it was moved aside and this show starts with no invites");
            Vec::new()
        }
    }
}

/// Write every invite, atomically. Says why when it could not.
pub fn write(path: &Path, invites: &[Record]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&File { invites: invites.to_vec() }).unwrap_or_default();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    owner_only(&tmp);
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(unix)]
fn owner_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn owner_only(_: &Path) {
    // Windows: the file inherits the folder's access list, which for a
    // config under the user's profile is that user and the administrators.
}
