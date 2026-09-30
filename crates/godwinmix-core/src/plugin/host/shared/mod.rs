//! A device or a stream opened and decoded once, read by every source that
//! uses it, in this mixer or in another one on the same machine.
//!
//! ```text
//!   mixer A                                     mixer B
//!   source cam ─┐                               source cam ─┐
//!               │ reads                                     │ reads
//!               ▼                                           ▼
//!         gmxbussrc ◄──── frame bus: camera:<device> ────► gmxbussrc
//!                                  ▲
//!   feed (whoever holds the claim) │
//!   plugin process ─► unixfd ─► normaliser ─► gmxbussink
//! ```
//!
//! Every shared source reads the bus, the owner's own included, so which
//! mixer owns the device is invisible to the picture: when the owner goes,
//! a reader takes the claim, starts the plugin itself, and publishes under the
//! same name, and every reader's `gmxbussrc` picks the new owner up by itself.
//!
//! ```text
//!   mod.rs     whether a source is shared, and where the registry is
//!   name.rs    the bus name a provide's params make
//!   source.rs  SharedSource: the `Source` the mixer holds
//!   owner.rs   the thread that takes the claim when nobody holds it
//!   feed.rs    the plugin process and the pipeline that publishes it
//!   reader.rs  the reading pipeline, and what it measures
//! ```
//!
//! The decision is made here, from the manifest's `share` key, and never in a
//! plugin: a third party camera gets it by declaring which param names its
//! device. Windows has no cross process transport yet, so there every source
//! opens its own device, as before.

mod feed;
mod name;
mod owner;
mod reader;
mod retime;
mod source;

pub use name::bus_name;
pub use reader::{Tracks, Watch};
pub use source::SharedSource;

use crate::config::Params;
use godwinmix_framebus::BusName;
use std::path::PathBuf;

/// Set to `off` to open every device once per source again, as before the
/// frame bus. For measuring what sharing saves, and a way back if it misbehaves.
pub const SWITCH_ENV: &str = "GODWINMIX_FRAMEBUS";

/// Whether sources may share on this machine at all.
pub fn enabled() -> bool {
    if godwinmix_framebus::available().is_err() {
        return false;
    }
    !matches!(
        std::env::var(SWITCH_ENV).ok().as_deref().map(str::trim),
        Some("off" | "0" | "false" | "no")
    )
}

/// The registry directory: the station's (`GODWINMIX_BUS_DIR`) when it set
/// one, otherwise `bus` under this user's GodwinMix home, one per machine and
/// user, so two mixers started by hand find each other.
pub fn bus_dir() -> PathBuf {
    match std::env::var_os(godwinmix_framebus::registry::DIR_ENV).filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => godwinmix_host::marketplace::home_dir().join("bus"),
    }
}

/// Where a shared source's frames are: its name, and the registry it is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub name: BusName,
    pub dir: PathBuf,
}

/// Where a source of `type_id` with `params` shares, or `None` when it is
/// opened on its own: the provide declares no `share`, the bus is off, or the
/// params name nothing to share (a channel source that listens).
///
/// A provide's `scope` params, when they are set, put the name in a registry
/// of its own under the station's, so a stream called `live/main` on one
/// channel server is never taken for another server's `live/main`.
pub fn plan(type_id: &str, params: &Params) -> Option<Place> {
    if !enabled() {
        return None;
    }
    let (plugin, provide) = type_id.split_once('/')?;
    let installed = crate::plugin::loader::get(plugin)?;
    let share = installed.manifest.provide(provide)?.share.clone()?;
    let json = super::source::params_json(params);
    let name = match bus_name(plugin, &share, &json) {
        Ok(name) => name?,
        Err(e) => {
            tracing::warn!(%type_id, error = %e, "these params make no frame bus name; opening the device unshared");
            return None;
        }
    };
    let scope = share.scope_values(&json);
    let dir = match scope.iter().all(String::is_empty) {
        true => bus_dir(),
        false => bus_dir().join(format!("at-{}", name::slug(&scope.join("|")))),
    };
    Some(Place { name, dir })
}

/// Make the element names known in this process. Safe to call again.
pub fn register_elements() -> anyhow::Result<()> {
    static DONE: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    DONE.get_or_init(|| godwinmix_framebus::gst::register().map_err(|e| e.to_string()))
        .clone()
        .map_err(|e| anyhow::anyhow!("the frame bus elements would not register: {e}"))
}
