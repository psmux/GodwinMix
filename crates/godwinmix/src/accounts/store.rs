//! The accounts file: `<stem>.users.toml` beside the config.
//!
//! Beside rather than inside, for the reason the device file is: the config
//! is a person's file with a comment on every line, and this one is rewritten
//! whole whenever somebody signs in. It holds no password and no session
//! secret. A password is kept as an argon2id hash and a session as the
//! SHA-256 of its secret, so the file in a backup lets nobody in.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use godwinmix_protocol::accounts::{Role, SessionInfo, UserInfo};
use godwinmix_protocol::scope::{ConfirmPolicy, TokenSafety};
use serde::{Deserialize, Serialize};

/// One account as it is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub name: String,
    pub role: Role,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub confirm: ConfirmPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety: Option<TokenSafety>,
    pub created: String,
    /// The argon2id PHC string. Never sent anywhere.
    pub password: String,
}

impl User {
    pub fn public(&self, sessions: usize) -> UserInfo {
        UserInfo {
            name: self.name.clone(),
            role: self.role,
            disabled: self.disabled,
            confirm: self.confirm,
            safety: self.safety,
            created: self.created.clone(),
            sessions,
        }
    }
}

/// One signed in browser as it is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Public and short, to name it in a list.
    pub id: String,
    pub user: String,
    /// SHA-256 of the secret in the cookie, lower case hex.
    pub sha256: String,
    pub created: String,
    /// Seconds since the Unix epoch.
    pub expires: u64,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub device: String,
}

impl Session {
    pub fn public(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.clone(),
            user: self.user.clone(),
            created: self.created.clone(),
            expires: rfc3339(self.expires),
            address: self.address.clone(),
            device: self.device.clone(),
            last_seen: None,
            current: false,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct File {
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default)]
    pub sessions: Vec<Session>,
}

/// `godwinmix.users.toml` for `godwinmix.toml`.
pub fn path_beside(config: &Path) -> PathBuf {
    let mut name = config.file_stem().unwrap_or_default().to_os_string();
    name.push(".users.toml");
    config.with_file_name(name)
}

/// Modification time and length: a change in either means somebody else
/// wrote it, the CLI or another process of a station.
pub type Stamp = (Option<std::time::SystemTime>, u64);

pub fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok(), meta.len()))
}

/// The file, or an empty one when there is none yet. A file that does not
/// parse is an error, so that saving over it does not lose anybody.
pub fn load(path: &Path) -> Result<File> {
    if !path.exists() {
        return Ok(File::default());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("reading {}", path.display()))
}

/// Write it through a temporary file and a rename, owner only, and owned by
/// whoever owns the folder: `sudo godwinmix users add` on a server must not
/// leave a file the service's own user cannot read.
pub fn save(path: &Path, file: &File) -> Result<()> {
    let body = toml::to_string_pretty(file).context("the accounts would not serialise")?;
    let text = format!(
        "# GodwinMix accounts and signed in sessions. Managed by the mixer: use\n\
         # Mixer settings > Users, or `godwinmix users`, rather than editing it.\n\
         # Passwords are argon2id hashes and sessions are SHA-256 digests; nothing\n\
         # here signs anybody in. Delete the file to remove every account.\n\n{body}"
    );
    godwinmix_core::config::edit::write_atomic(path, text.as_bytes())?;
    restrict(path);
    Ok(())
}

#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    let folder = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let (Ok(mine), Ok(theirs)) = (std::fs::metadata(path), std::fs::metadata(folder)) else { return };
    if (mine.uid(), mine.gid()) != (theirs.uid(), theirs.gid()) {
        // Only root may give a file away, and only root needs to.
        let _ = std::os::unix::fs::chown(path, Some(theirs.uid()), Some(theirs.gid()));
    }
}

#[cfg(not(unix))]
fn restrict(_path: &Path) {}

/// SHA-256 of a secret, lower case hex.
pub fn digest(secret: &str) -> String {
    let hash = ring::digest::digest(&ring::digest::SHA256, secret.as_bytes());
    hash.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn rfc3339(secs: u64) -> String {
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
    godwinmix_core::observe::logs::rfc3339(&at)
}
