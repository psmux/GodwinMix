//! Accounts and their sessions: the store `Tokens` asks about a secret it
//! does not know, after the device tokens.
//!
//! One file per config, `<stem>.users.toml` beside it (see `store`). Under a
//! station every show reads the station's file, whose path rides in
//! [`PATH_ENV`], as the device file's does. Any process may write it: the
//! core when somebody signs in, the CLI on a server. Each reads it again when
//! it changed, on an unknown secret at once and otherwise at most once every
//! [`RECHECK`], so an account disabled from the command line is shut out of a
//! running mixer within a second.

mod limit;
mod manage;
pub mod calls;
pub mod cookie;
pub mod http;
pub mod password;
mod registry;
pub mod seed;
mod signin;
pub mod store;
#[cfg(test)]
mod tests;

pub use limit::Limiter;
pub use signin::{Refused, SignedIn};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::scope::Token;
use parking_lot::Mutex;

/// Where a show finds its station's accounts file.
pub const PATH_ENV: &str = "GODWINMIX_ACCOUNTS_FILE";

/// How stale a process's copy of the file may be.
pub const RECHECK: Duration = Duration::from_secs(1);

/// How long a sign in lasts. A month, so nobody is signed out in the middle
/// of a show they signed in for; a connection open when it runs out stays
/// open.
pub const SESSION_DAYS: u64 = 30;

/// A session's `Token::secret` is this and the digest, never the secret.
pub(crate) const SECRET_PREFIX: &str = "session-sha256:";

#[derive(Debug)]
pub struct Accounts {
    path: Option<PathBuf>,
    pub(crate) state: Mutex<State>,
    /// Failed sign ins, per address and per username.
    pub limiter: Limiter,
}

#[derive(Debug, Default)]
pub(crate) struct State {
    pub(crate) file: store::File,
    seen: Option<store::Stamp>,
    checked: Option<Instant>,
    /// Why the file on disk would not parse. Nothing is written over it until
    /// somebody fixes it, so no account is lost by accident.
    broken: Option<String>,
    /// When each session was last seen by this process, by digest.
    pub(crate) seen_at: HashMap<String, std::time::SystemTime>,
}

/// The store for a core started with `config`: the station's file when this
/// is a show under one, else the one beside `config`. An embedded core with
/// no config file keeps its accounts in memory only.
pub fn for_config(config: &Path) -> Arc<Accounts> {
    let from_env = std::env::var_os(PATH_ENV).filter(|p| !p.is_empty()).map(PathBuf::from);
    let path = from_env.or_else(|| (!config.as_os_str().is_empty()).then(|| store::path_beside(config)));
    Arc::new(Accounts::open(path))
}

/// The station's store: [`for_config`], remembered so every show it starts is
/// pointed at the same file.
pub fn for_station(config: &Path) -> Arc<Accounts> {
    let accounts = for_config(&std::path::absolute(config).unwrap_or_else(|_| config.to_path_buf()));
    if let Some(path) = &accounts.path {
        let _ = STATION_FILE.set(path.clone());
    }
    accounts
}

static STATION_FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The file a show started by this station reads, for `child::command`.
pub fn station_file() -> Option<&'static Path> {
    STATION_FILE.get().map(PathBuf::as_path)
}

/// The concrete store behind a `Tokens`, for the management calls and the
/// sign in routes.
pub fn of(tokens: &godwinmix_protocol::scope::Tokens) -> Option<&Accounts> {
    tokens.accounts()?.as_any().downcast_ref::<Accounts>()
}

impl Accounts {
    pub fn open(path: Option<PathBuf>) -> Self {
        let accounts = Accounts { path, state: Mutex::new(State::default()), limiter: Limiter::default() };
        accounts.refresh(&mut accounts.state.lock(), true);
        accounts
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Read the file again when it changed since it was last read. Without
    /// `force`, only when [`RECHECK`] has passed since the last look.
    pub(crate) fn refresh(&self, state: &mut State, force: bool) {
        let Some(path) = &self.path else { return };
        if !force && state.checked.is_some_and(|at| at.elapsed() < RECHECK) {
            return;
        }
        state.checked = Some(Instant::now());
        let now = store::stamp(path);
        if now == state.seen {
            return;
        }
        state.seen = now;
        match store::load(path) {
            Ok(file) => {
                state.file = file;
                state.broken = None;
            }
            // The copy in memory stays: a hand broken file must not lock
            // everybody out, nor let a removed account back in.
            Err(e) => {
                tracing::warn!("{e:#}. Accounts stay as they were; fix or remove the file.");
                state.broken = Some(format!("{e:#}"));
            }
        }
    }

    /// Write the file out, dropping sessions that ran out a day ago, and remember
    /// it as this process left it so it is not read straight back.
    pub(crate) fn persist(&self, state: &mut State) -> Result<(), RpcError> {
        // A day's grace, so a connection that was open when its session ran
        // out is not cut the moment somebody else signs in.
        let now = store::now_secs();
        state.file.sessions.retain(|s| s.expires + 86_400 > now);
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(why) = &state.broken {
            return Err(RpcError::not_in_state(format!(
                "{why}. Nothing changed, so the accounts in it are not lost. Fix the file, or move it aside to start with no accounts, and try again."
            ))
            .with("path", path.display().to_string()));
        }
        store::save(path, &state.file).map_err(|e| {
            RpcError::internal(format!("{e:#}. Nothing changed. Check that the folder holding {} can be written to.", path.display()))
        })?;
        state.seen = store::stamp(path);
        Ok(())
    }
}

/// The credential a session stands for: its account's role as the one scope,
/// the ladder below implied, and the account's confirm policy and safety.
pub(crate) fn token_for(user: &store::User, session: &store::Session) -> Token {
    Token {
        id: user.name.clone(),
        secret: format!("{SECRET_PREFIX}{}", session.sha256),
        scopes: vec![user.role.scope()],
        confirm: user.confirm,
        safety: user.safety,
        ..Token::open()
    }
}
