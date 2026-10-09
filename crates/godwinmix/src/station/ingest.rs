//! The ingest plugin, started by the station whenever it arrives.
//!
//! The station runs the ingest plugin once for the machine: it serves the
//! channels and is the direct host for every show without compositing. At
//! start the station starts it if it is installed. A `plugin.add` (or an
//! update, an enable, a reload) is answered by a show, which installs into
//! the shared plugins directory and leaves the ingest plugin alone, so the
//! station looks for it itself: from the moment such a call passes through,
//! once a second, until the plugin runs or twenty minutes go by (a release
//! to download or a crate to compile can take that long). The plugin starts
//! with the settings the station already laid over its own, the channel
//! table and the direct table among them, so nothing waits for a restart.

use super::state::Station;
use godwinmix_core::config::Params;
use godwinmix_core::plugin::{loader, supervisor::Supervisor};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// How long one plugin call keeps the station looking.
const LOOK_FOR: Duration = Duration::from_secs(20 * 60);
const EVERY: Duration = Duration::from_secs(1);
/// How long a start waits for the first direct table.
const TABLE_WAIT: Duration = Duration::from_secs(10);

static BUDGETS: OnceLock<BTreeMap<String, Params>> = OnceLock::new();
static LOOKING: AtomicBool = AtomicBool::new(false);

/// The operator's `[plugins.<name>]` settings, read once at start, which a
/// plugin found later is read with.
pub fn configure(budgets: BTreeMap<String, Params>) {
    let _ = BUDGETS.set(budgets);
}

/// Whether a call can bring the ingest plugin in or back.
pub fn may_bring_it(method: &str) -> bool {
    matches!(method, "plugin.add" | "plugin.update" | "plugin.enable" | "plugin.reload")
}

/// A plugin call passed through to a show: look for the ingest plugin
/// until it runs. One look at a time however many calls pass.
pub fn after(st: &Arc<Station>, method: &str) {
    if !may_bring_it(method) {
        return;
    }
    let Some(plugins) = st.direct.plugins().cloned() else { return };
    if plugins.is_running(crate::channels::PLUGIN) || LOOKING.swap(true, Ordering::SeqCst) {
        return;
    }
    let st = Arc::downgrade(st);
    let spawned = std::thread::Builder::new().name("ingest-arrives".into()).spawn(move || {
        let until = Instant::now() + LOOK_FOR;
        while Instant::now() < until && !st.upgrade().is_none_or(|st| try_start(&st, &plugins)) {
            std::thread::sleep(EVERY);
        }
        LOOKING.store(false, Ordering::SeqCst);
    });
    if spawned.is_err() {
        LOOKING.store(false, Ordering::SeqCst);
        warn!("no thread to look for the ingest plugin; it starts at the next station start");
    }
}

/// Start the ingest plugin if it is installed and well. True when it runs.
fn try_start(st: &Station, plugins: &Supervisor) -> bool {
    if plugins.is_running(crate::channels::PLUGIN) {
        return true;
    }
    let empty = BTreeMap::new();
    let budgets = BUDGETS.get().unwrap_or(&empty);
    let Some(found) = loader::scan(budgets).into_iter().find(|p| p.name() == crate::channels::PLUGIN && p.live()) else {
        return false;
    };
    loader::insert(found);
    match start(st, plugins) {
        Ok(()) => {
            info!("the ingest plugin arrived and was started with the channel and direct tables");
            // The channels are the station's, so the show that took the
            // install has none to give a default to: a station with no
            // channels gets Live here, as a core on its own does at install.
            if let Some(channels) = st.channels.get() {
                channels.ensure_default();
            }
            true
        }
        Err(e) => {
            warn!(error = %format!("{e:#}"), "the ingest plugin arrived but would not start; the station tries again in a second");
            false
        }
    }
}

/// Start the ingest plugin with the direct table in its settings. The table
/// is built on a thread of its own, so the first one may still be on its
/// way: wait for it (a few seconds at most) before starting. And a table
/// handed while the plugin was still starting reached nobody, since only a
/// running plugin is configured, so hand it again once it runs.
pub fn start(st: &Station, plugins: &Supervisor) -> anyhow::Result<()> {
    let asked = st.direct.hand_over();
    if !st.direct.wait_handed(asked, TABLE_WAIT) {
        warn!("the direct table was not ready in time; the ingest plugin starts without it and is handed it when it is");
    }
    plugins.start(crate::channels::PROVIDE)?;
    st.direct.hand_over();
    Ok(())
}
