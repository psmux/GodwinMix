//! Device tokens: the registry `Tokens` asks about a secret it does not know.
//!
//! One file per machine, `<stem>.devices.toml` beside the config the station
//! was started with. Under a station every show reads the station's file (the
//! path rides in [`PATH_ENV`]), because a phone's call is checked twice: once
//! by the station at the door and again by the show it is relayed to. Only
//! the process that answers `token.create` or `token.revoke` writes; the
//! others notice the file changed and read it again, on the next unknown
//! secret at once and otherwise at most once every [`RECHECK`].

mod calls;
mod registry;
mod store;
#[cfg(test)]
mod tests;

pub use calls::call;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::scope::{constant_time_eq, Token};
use parking_lot::Mutex;
use store::Record;

/// Where a show finds its station's device file.
pub const PATH_ENV: &str = "GODWINMIX_DEVICE_TOKENS";

/// How stale a process's copy of the file may be when a known token is
/// presented. A revoke answered by another process reaches this one within
/// this long; one answered here is in force at once.
pub const RECHECK: Duration = Duration::from_secs(1);

/// A device token's `secret` is this and the digest, never the secret itself,
/// which this core does not keep.
pub(super) const SECRET_PREFIX: &str = "device-sha256:";

#[derive(Debug)]
pub struct Devices {
    path: Option<PathBuf>,
    pub(super) state: Mutex<State>,
}

#[derive(Debug, Default)]
pub(super) struct State {
    pub(super) records: Vec<Record>,
    seen: Option<store::Stamp>,
    checked: Option<Instant>,
    /// Why the file as it is on disk would not parse. Nothing is written
    /// over it until somebody fixes it, so no phone is lost by accident.
    broken: Option<String>,
}

/// The registry for a core started with `config`: the station's file when
/// this is a show under one, else the one beside `config`. An embedded core
/// with no config file keeps its device tokens in memory only.
pub fn for_config(config: &Path) -> Arc<Devices> {
    let from_env = std::env::var_os(PATH_ENV).filter(|p| !p.is_empty()).map(PathBuf::from);
    let path = from_env.or_else(|| (!config.as_os_str().is_empty()).then(|| store::path_beside(config)));
    Arc::new(Devices::open(path))
}

/// The station's registry: [`for_config`], with the file it settled on
/// remembered so every show it starts is pointed at the same one.
pub fn for_station(config: &Path) -> Arc<Devices> {
    let devices = for_config(&std::path::absolute(config).unwrap_or_else(|_| config.to_path_buf()));
    if let Some(path) = &devices.path {
        let _ = STATION_FILE.set(path.clone());
    }
    devices
}

static STATION_FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The file a show started by this station reads, for `child::command`.
pub fn station_file() -> Option<&'static Path> {
    STATION_FILE.get().map(PathBuf::as_path)
}

impl Devices {
    pub fn open(path: Option<PathBuf>) -> Self {
        let devices = Devices { path, state: Mutex::new(State::default()) };
        devices.refresh(&mut devices.state.lock(), true);
        devices
    }

    /// Read the file again when it changed since it was last read. Without
    /// `force`, only when [`RECHECK`] has passed since the last look.
    pub(super) fn refresh(&self, state: &mut State, force: bool) {
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
            Ok(records) => {
                state.records = records;
                state.broken = None;
            }
            // The copy in memory stays: a hand broken file must not lock
            // every phone out, nor let a revoked one back in.
            Err(e) => {
                tracing::warn!("{e:#}. Device tokens stay as they were; fix or remove the file.");
                state.broken = Some(format!("{e:#}"));
            }
        }
    }

    /// Write the records out, and remember the file as this process left it
    /// so it is not read straight back.
    pub(super) fn persist(&self, state: &mut State) -> Result<(), RpcError> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(why) = &state.broken {
            return Err(RpcError::not_in_state(format!(
                "{why}. Nothing changed, so the device tokens in it are not lost. Fix the file, \
                 or move it aside to start an empty list, and try again."
            ))
            .with("path", path.display().to_string()));
        }
        store::save(path, &state.records).map_err(|e| {
            RpcError::internal(format!("{e:#}. Nothing changed. Check that the folder holding {} can be written to.", path.display()))
        })?;
        state.seen = store::stamp(path);
        Ok(())
    }

    pub(super) fn lookup(&self, state: &State, digest: &str) -> Option<Token> {
        let record = state.records.iter().find(|r| constant_time_eq(r.sha256.as_bytes(), digest.as_bytes()))?;
        Some(token_for(record))
    }
}

/// The credential a device token stands for. One scope, the ladder below it
/// implied, and nothing a person at a desk could not also be given.
pub(super) fn token_for(record: &Record) -> Token {
    Token {
        id: record.id.clone(),
        secret: format!("{SECRET_PREFIX}{}", record.sha256),
        scopes: vec![record.scope],
        ..Token::open()
    }
}
