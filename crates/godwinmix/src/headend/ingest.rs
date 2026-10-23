//! The ingest plugin, which runs every stream, installed at start when the
//! plugins folder has none.
//!
//! `plugin.add` is a show's method: a station relays it to its first show,
//! and a headend runs none, so `gmx plugin add ingest` has nobody to answer
//! it. Teaching the station to answer `plugin.add` itself would move the
//! whole install path (its hooks, its tasks, the singletons it starts) out
//! of the show for one case. A headend needs exactly one plugin, the one
//! that ships beside the binary, so it installs that one through the same
//! `loader::install` and trust check `plugin.add` uses, and starts it the
//! way the station starts an ingest plugin that arrives.

use super::Settings;
use crate::station::state::Station;
use godwinmix_core::plugin::{first_party, loader};
use std::sync::Arc;
use tracing::{info, warn};

pub async fn ensure(st: &Arc<Station>, s: &Settings) {
    let name = crate::channels::PLUGIN;
    if loader::get(name).is_some() {
        return;
    }
    let Some(from) = first_party::find(name) else {
        let looked: Vec<String> = first_party::places(name).iter().map(|p| p.display().to_string()).collect();
        return warn!(looked = ?looked, "headend: no ingest plugin is installed and none ships beside this binary; no stream runs until one is installed");
    };
    info!(from = %from.display(), "headend: installing the ingest plugin that ships beside this binary");
    let opts = loader::InstallOptions { allow_unsigned: s.allow_unsigned, only: s.marketplaces_only.clone(), ..Default::default() };
    let installed = tokio::task::spawn_blocking(move || loader::install(name, &opts)).await;
    match installed {
        Ok(Ok(p)) => {
            info!(version = p.version(), trust = p.trust.label(), "headend: the ingest plugin is installed");
            crate::station::ingest::after(st, "plugin.add");
        }
        Ok(Err(e)) => warn!(error = %format!("{e:#}"), "headend: the ingest plugin would not install; no stream runs until it does"),
        Err(e) => warn!(error = %e, "headend: the ingest plugin's install did not finish"),
    }
}
