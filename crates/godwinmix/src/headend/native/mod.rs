//! The headend channels the station keeps itself, set up on the headend
//! screen or by `headend.channel.*`, with no Flussonic file behind them.
//!
//! ```text
//!   headend.channel.* ──► store (headend.json) ──compile──► ShowPlan ──keeper──► direct shows
//!   headend.import ─────┘                                     ▲
//!   followed file ──import──► ShowPlan ──keeper (the file's)──┘  less what the store holds
//! ```
//!
//! A channel compiles to the plan an imported stream does, and one keeper
//! runs each list, so a channel made here is the same show, with the same
//! HLS output and the same Flussonic compatible URLs, as one imported.
//!
//! A stream of a followed file that is edited here leaves the file's hands:
//! the store keeps it from then on, the file's keeper hands its show over
//! without a restart, and later edits to that stream in the file are not
//! applied until `headend.channel.follow` gives it back.

pub mod calls;
mod check;
pub mod compile;
mod edit;
mod export;
mod follow;
mod ops;
pub mod scan;
mod scan_read;
pub mod store;
pub mod table;
#[cfg(test)]
mod tests;

use super::keep::{Keeper, Outcome};
use crate::station::state::Station;
use godwinmix_flussonic::import::ShowPlan;
use parking_lot::Mutex;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use store::Store;

/// How often a channel that could not be made is tried again.
const AGAIN: Duration = Duration::from_secs(30);

pub struct Native {
    st: Weak<Station>,
    dir: PathBuf,
    pub keep: Keeper,
    pub store: Mutex<Store>,
    /// One edit at a time, so two pages saving at once do not lose either.
    pub edits: tokio::sync::Mutex<()>,
    started: AtomicBool,
}

impl Native {
    /// The station's, read from `headend.json` the first time it is asked for.
    pub fn of(st: &Arc<Station>) -> Arc<Native> {
        st.native
            .get_or_init(|| {
                let dir = st.registry.lock().data_dir().to_path_buf();
                Arc::new(Native { st: Arc::downgrade(st), store: Mutex::new(store::load(&dir)), dir, keep: Keeper::new(st, super::saved::NATIVE), edits: tokio::sync::Mutex::new(()), started: AtomicBool::new(false) })
            })
            .clone()
    }

    pub fn station(&self) -> Option<Arc<Station>> {
        self.st.upgrade()
    }

    /// The names the followed file's keeper leaves alone: every channel kept
    /// here, and every stream taken over or removed here.
    pub fn owned(&self) -> BTreeSet<String> {
        let s = self.store.lock();
        s.channels.iter().map(|c| c.name.clone()).chain(s.detached.keys().cloned()).collect()
    }

    pub fn save(&self) -> Result<(), String> {
        store::save(&self.dir, &self.store.lock()).map_err(|e| format!("could not write {}: {e}", store::path(&self.dir).display()))
    }

    /// Every channel's plan. One that will not compile (a profile removed by
    /// hand from the file) is left out and said.
    pub fn plans(&self) -> Vec<ShowPlan> {
        let s = self.store.lock();
        let profiles = compile::all_profiles(&s.profiles);
        s.channels
            .iter()
            .filter_map(|c| compile::plan(c, &profiles).map_err(|e| tracing::warn!(channel = %c.name, error = %e, "headend: a channel is not run")).ok())
            .collect()
    }

    pub async fn apply(&self) -> Outcome {
        self.keep.apply(self.plans()).await
    }

    /// The channels in Flussonic's language, for the Flussonic API.
    pub fn streams(&self) -> Vec<godwinmix_flussonic::model::Stream> {
        let text = export::text(&self.store.lock()).text;
        godwinmix_flussonic::Document::parse(&text).map(|d| d.config.streams).unwrap_or_default()
    }

    /// Run the channels, once per station, and try again every 30 s those
    /// the station could not make.
    pub fn start(self: &Arc<Self>) {
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        if self.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let me = Arc::downgrade(self);
        rt.spawn(async move {
            loop {
                let Some(n) = me.upgrade() else { return };
                if !n.store.lock().channels.is_empty() || !n.keep.running.lock().is_empty() {
                    n.apply().await;
                }
                drop(n);
                tokio::time::sleep(AGAIN).await;
            }
        });
    }
}

/// Start the station's own channels, if it keeps any. Called once the
/// station has its direct host.
pub fn start(st: &Arc<Station>) {
    let dir = st.registry.lock().data_dir().to_path_buf();
    if store::path(&dir).exists() || dir.join(super::saved::NATIVE).exists() {
        Native::of(st).start();
    }
}
