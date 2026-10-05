//! The device token file: `<stem>.devices.toml` beside the config.
//!
//! Beside rather than inside, for the reason the channels file is: the config
//! is a person's file with a comment on every line, and this one is rewritten
//! whole every time a phone is added or taken away. It holds no secret. Each
//! token is kept as the SHA-256 of its secret, so the file in a support bundle
//! or a backup lets nobody in.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use godwinmix_protocol::devices::DeviceToken;
use godwinmix_protocol::scope::Scope;
use serde::{Deserialize, Serialize};

/// One device token as it is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub label: String,
    pub scope: Scope,
    pub created: String,
    /// SHA-256 of the secret, lower case hex.
    pub sha256: String,
}

impl Record {
    pub fn public(&self) -> DeviceToken {
        DeviceToken { id: self.id.clone(), label: self.label.clone(), scope: self.scope, created: self.created.clone() }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    devices: Vec<Record>,
}

/// The file's path for a config: `godwinmix.devices.toml` for
/// `godwinmix.toml`.
pub fn path_beside(config: &Path) -> PathBuf {
    let mut name = config.file_stem().unwrap_or_default().to_os_string();
    name.push(".devices.toml");
    config.with_file_name(name)
}

/// What the file says when it was last written: its modification time and
/// length. A change in either means another process wrote it.
pub type Stamp = (Option<std::time::SystemTime>, u64);

pub fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok(), meta.len()))
}

/// Every record, or none when there is no file yet. A file that does not
/// parse is an error, so that saving over it does not quietly lose somebody's
/// phones.
pub fn load(path: &Path) -> Result<Vec<Record>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: File = toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
    Ok(file.devices)
}

/// Write them through a temporary file and a rename, owner only where the
/// platform has the notion.
pub fn save(path: &Path, records: &[Record]) -> Result<()> {
    let body = toml::to_string_pretty(&File { devices: records.to_vec() }).context("the device tokens would not serialise")?;
    let text = format!(
        "# Device tokens, made with token.create (Open on another device, in the\n\
         # page's Help menu). Only a SHA-256 of each secret is here; the secret was\n\
         # shown once. token.revoke takes one away.\n\n{body}"
    );
    godwinmix_core::config::edit::write_atomic(path, text.as_bytes())?;
    restrict(path);
    Ok(())
}

#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_path: &Path) {}

/// SHA-256 of a secret, lower case hex.
pub fn digest(secret: &str) -> String {
    let hash = ring::digest::digest(&ring::digest::SHA256, secret.as_bytes());
    hash.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}
