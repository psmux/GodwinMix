//! Shows without compositing: the station's half of the direct host.
//!
//! A direct show has no process. Its record (input, outputs) is in the list
//! of shows, and the whole table of them is handed to the direct host in the
//! ingest plugin the way the channel table is: laid over the plugin's
//! settings as `direct` and pushed with `configure`. What the host says back
//! (`event/direct.*`) is kept here per show and read by `show.list`,
//! `show.stats` and `event/show.health`. The shapes are
//! `dev/plans/wave4-direct-table.md`.
//!
//! ```text
//!   show.* ──► registry ──dirty──► handover thread ──set_extra + configure──► direct host
//!                 ▲                    (plans renditions)                         │
//!                 └──── seen ◄── intake thread ◄── event/direct.* ◄───────────────┘
//! ```
//!
//! Both threads are this module's own, so nothing here blocks a handler, the
//! plugin pump or the runtime.

mod edit;
mod health;
mod intake;
pub mod outputs;
mod plan;
mod seen;
mod table;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_intake;
mod view;

pub use edit::{add as add_output, set as set_output};
pub use plan::room;
pub use seen::Seen;
pub use view::{feed_source, INPUT_SOURCE};
pub use intake::take as take_event;

use super::state::Station;
use crate::channels::transcode::Transcode;
use godwinmix_core::plugin::supervisor::Supervisor;
use parking_lot::{Condvar, Mutex};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// The key the table is laid over the plugin's settings under.
pub const EXTRA: &str = "direct";

pub struct Direct {
    plugins: OnceLock<Arc<Supervisor>>,
    /// What the host last said about each show.
    pub(crate) seen: Mutex<BTreeMap<String, Seen>>,
    /// The renditions of direct outputs, planned and admitted like a
    /// channel's, against the station's governor.
    pub(crate) transcode: Transcode,
    /// How many times a table was asked for, and how many were handed.
    gens: Mutex<(u64, u64)>,
    handed: Condvar,
    wake: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl Direct {
    pub fn new(data_dir: Option<std::path::PathBuf>) -> Direct {
        Direct {
            plugins: OnceLock::new(),
            seen: Mutex::new(BTreeMap::new()),
            transcode: Transcode::new(data_dir),
            gens: Mutex::new((0, 0)),
            handed: Condvar::new(),
            wake: Mutex::new(None),
        }
    }

    /// Give the direct host's plugin to the station, before it starts, and
    /// start the two threads.
    pub fn attach(st: &Arc<Station>, plugins: Arc<Supervisor>) {
        st.direct.transcode.use_governor(st.render.governor().clone());
        let _ = st.direct.plugins.set(plugins.clone());
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        *st.direct.wake.lock() = Some(tx);
        table::start(Arc::downgrade(st), rx);
        intake::start(st, &plugins);
        st.direct.hand_over();
    }

    pub(crate) fn plugins(&self) -> Option<&Arc<Supervisor>> {
        self.plugins.get()
    }

    /// Ask for the table to be built and handed over. Many asks in a row
    /// make one table. Answers the ask's number, for `wait_handed`.
    pub fn hand_over(&self) -> u64 {
        let asked = {
            let mut g = self.gens.lock();
            g.0 += 1;
            g.0
        };
        if let Some(tx) = self.wake.lock().as_ref() {
            let _ = tx.send(());
        }
        asked
    }

    /// Wait until a table at least as new as ask `asked` was handed over,
    /// for at most `wait`. False when it was not, or no thread runs.
    pub fn wait_handed(&self, asked: u64, wait: Duration) -> bool {
        if self.wake.lock().is_none() {
            return false;
        }
        let mut g = self.gens.lock();
        let deadline = std::time::Instant::now() + wait;
        while g.1 < asked {
            if self.handed.wait_until(&mut g, deadline).timed_out() {
                return g.1 >= asked;
            }
        }
        true
    }

    fn asked(&self) -> u64 {
        self.gens.lock().0
    }

    fn done(&self, gen: u64) {
        let mut g = self.gens.lock();
        g.1 = g.1.max(gen);
        self.handed.notify_all();
    }

    /// Forget what the host said about a show that went.
    pub fn forget(&self, id: &str) {
        self.seen.lock().remove(id);
    }
}
