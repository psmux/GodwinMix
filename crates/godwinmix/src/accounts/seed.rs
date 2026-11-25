//! The first admin for an unattended install, from the environment.
//!
//! `GODWINMIX_ADMIN_USER` names the account and `GODWINMIX_ADMIN_PASSWORD_FILE`
//! names a file holding its password. Read once, when the mixer starts with
//! no account at all: the account is made, and the password file is deleted
//! when the mixer may delete it. With an account already there both are
//! ignored, so leaving them in a unit file does nothing on later starts and
//! never puts back a password somebody changed.

use std::path::Path;

use godwinmix_protocol::accounts::{Role, UserAddRequest};
use godwinmix_protocol::scope::ConfirmPolicy;

use super::Accounts;

pub const USER_ENV: &str = "GODWINMIX_ADMIN_USER";
pub const PASSWORD_FILE_ENV: &str = "GODWINMIX_ADMIN_PASSWORD_FILE";

/// Make the first admin when the environment asks for one and there is no
/// account yet. Says what it did in the log; never stops the mixer starting.
pub fn from_env(accounts: &Accounts) {
    let var = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let (user, file) = (var(USER_ENV), var(PASSWORD_FILE_ENV));
    let (Some(user), Some(file)) = (user.clone(), file.clone()) else {
        if user.is_some() || file.is_some() {
            tracing::warn!("{USER_ENV} and {PASSWORD_FILE_ENV} go together and only one is set, so no admin account was made");
        }
        return;
    };
    match seed(accounts, &user, Path::new(&file)) {
        Ok(true) => tracing::info!(%user, "the first admin account was made from {USER_ENV}"),
        Ok(false) => tracing::debug!("accounts exist already, so {USER_ENV} is not used"),
        Err(e) => tracing::warn!("{e}. No admin account was made; add one with `godwinmix users add`."),
    }
}

/// True when the account was made, false when accounts already existed.
pub fn seed(accounts: &Accounts, user: &str, file: &Path) -> Result<bool, String> {
    if !accounts.list().is_empty() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(file).map_err(|e| format!("{PASSWORD_FILE_ENV} names {}, which could not be read: {e}", file.display()))?;
    let password = text.strip_suffix('\n').map(|t| t.strip_suffix('\r').unwrap_or(t)).unwrap_or(&text);
    let req = UserAddRequest { name: user.to_string(), role: Role::Admin, password: password.to_string(), confirm: ConfirmPolicy::None };
    accounts.add(req, &|_| false).map_err(|e| e.message)?;
    match std::fs::remove_file(file) {
        Ok(()) => tracing::info!(path = %file.display(), "the admin password file was read and deleted"),
        Err(e) => tracing::warn!(path = %file.display(), "the admin password file was read but could not be deleted ({e}); delete it yourself"),
    }
    Ok(true)
}
