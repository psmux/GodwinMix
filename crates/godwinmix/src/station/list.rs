//! `show.list`: every show, and for the running ones what is on air, what
//! goes out and what its process costs.
//!
//! A read never asks a show anything. What only a show's process can say
//! (on air, what its outputs send) and what its process costs are measured
//! by a sampler of the station's own, once a second, into a cache every
//! read is served from; everything else is the station's own state. The
//! sampler runs only while someone reads: it starts with the first
//! `show.list` and stops ten seconds after the last, so nothing runs unless
//! asked. The very first read waits for one round, so it never answers
//! zeros it could have measured.

use super::state::Station;
use futures_util::future::join_all;
use godwinmix_protocol::shows::{Show, ShowList, ShowState};
use godwinmix_protocol::{MixerStatus, OutputState};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const STATUS_WAIT: Duration = Duration::from_secs(1);
const EVERY: Duration = Duration::from_secs(1);
/// How long after the last read the sampler keeps going.
const LINGER: Duration = Duration::from_secs(10);

/// What a show's process said of itself at the last round.
#[derive(Debug, Clone, Default)]
struct Measured {
    on_air: Option<String>,
    programme_kbps: u64,
    cpu_millicores: u32,
    memory_mib: u64,
}

#[derive(Default)]
pub struct ListCache {
    measured: Mutex<BTreeMap<String, Measured>>,
    /// The last read, and whether a sampler task runs.
    read: Mutex<(Option<Instant>, bool)>,
    sampled: Mutex<Option<Instant>>,
}

impl Station {
    pub async fn list(self: &Arc<Self>) -> ShowList {
        let start = {
            let mut read = self.list_cache.read.lock();
            read.0 = Some(Instant::now());
            !std::mem::replace(&mut read.1, true)
        };
        if self.list_cache.sampled.lock().is_none() {
            self.sample().await;
        }
        if start {
            tokio::spawn(sampler(self.clone()));
        }
        let ids = self.registry.lock().ids();
        let measured = self.list_cache.measured.lock().clone();
        let shows = ids.iter().filter_map(|id| Some(fill(self.view(id)?, measured.get(id)))).collect();
        ShowList { shows, current: self.first() }
    }

    /// One round: every running show that composites asked for its status
    /// at once (a second at most), and every process read in one go.
    async fn sample(&self) {
        let ids: Vec<String> = self.registry.lock().records.iter().filter(|r| r.compositing).map(|r| r.id.clone()).collect();
        let statuses = join_all(ids.iter().map(|id| async move {
            match self.state_of(id) {
                Some(ShowState::Running) => self.status_of(id, STATUS_WAIT).await,
                _ => None,
            }
        }))
        .await;
        let children = self.children_now();
        let mut out = BTreeMap::new();
        for (id, status) in ids.iter().zip(statuses) {
            let mut m = Measured::default();
            if let Some(status) = status {
                m.on_air = status.scene.clone().or_else(|| status.program.clone());
                m.programme_kbps = programme_kbps(&status);
            }
            if let Some(s) = children.shows.get(id) {
                m.cpu_millicores = s.cpu_percent.map(super::usage::millicores).unwrap_or(0);
                m.memory_mib = s.rss_bytes.map(|b| b >> 20).unwrap_or(0);
            }
            out.insert(id.clone(), m);
        }
        *self.list_cache.measured.lock() = out;
        *self.list_cache.sampled.lock() = Some(Instant::now());
    }
}

/// Keep the cache current while someone reads, and stop after.
async fn sampler(st: Arc<Station>) {
    loop {
        tokio::time::sleep(EVERY).await;
        let idle = st.list_cache.read.lock().0.is_none_or(|t| t.elapsed() > LINGER);
        if idle || st.stopping.load(std::sync::atomic::Ordering::Relaxed) {
            st.list_cache.read.lock().1 = false;
            return;
        }
        st.sample().await;
    }
}

/// A show's view with what its process said of itself. A show without
/// compositing has its numbers from the direct host already.
fn fill(mut show: Show, m: Option<&Measured>) -> Show {
    if let (true, Some(m)) = (show.compositing, m) {
        if show.state == ShowState::Running {
            show.on_air = m.on_air.clone();
            show.programme_kbps = m.programme_kbps;
        }
        show.cpu_millicores = m.cpu_millicores;
        show.memory_mib = m.memory_mib;
    }
    show
}

fn programme_kbps(status: &MixerStatus) -> u64 {
    status
        .outputs
        .iter()
        .filter(|o| o.state == OutputState::Live)
        .filter_map(|o| o.extra.get("bitrate_kbps").and_then(serde_json::Value::as_u64))
        .sum()
}
