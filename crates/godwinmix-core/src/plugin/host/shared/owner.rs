//! The thread that makes a shared source the owner when nobody else is.
//!
//! Every 50 ms it asks for the claim, which is one `flock` that fails at once
//! while another process (or another source here) holds it. When it gets the
//! claim it starts the feed, and from then on it watches the feed and gives
//! the claim back if the feed fails, so a reader elsewhere can try. It is a
//! thread of its own because starting a plugin blocks for as long as the
//! plugin takes to say hello, and nothing the mixer runs may wait on that.

use super::feed::{Feed, Plan};
use super::reader::Watch;
use godwinmix_framebus::{Claim, Registry};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::{info, warn};

const TICK: Duration = Duration::from_millis(50);
/// After a feed fails, wait this long before trying again, so a reader in
/// another process with a working device gets the first chance.
const BACKOFF: Duration = Duration::from_secs(1);

/// What the owner thread shares with the source that started it.
#[derive(Default)]
pub struct Shared {
    pub feed: Mutex<Option<Feed>>,
    pub plan: Mutex<Option<Plan>>,
    stop: AtomicBool,
    /// Set by a restart: close the feed and open the device again from
    /// nothing on the next tick.
    reopen: AtomicBool,
    /// Times this source became the owner.
    pub takeovers: AtomicU64,
    /// How long the last takeover took, from finding the claim free to the
    /// plugin answering `start`.
    pub last_start_ms: AtomicU64,
}

pub struct Owner {
    pub shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Owner {
    pub fn spawn(plan: Plan, watch: Arc<Watch>) -> std::io::Result<Owner> {
        let shared = Arc::new(Shared::default());
        *shared.plan.lock() = Some(plan.clone());
        let run = shared.clone();
        let thread = std::thread::Builder::new()
            .name(format!("share {}", plan.name))
            .spawn(move || run_loop(&run, &watch))?;
        Ok(Owner { shared, thread: Some(thread) })
    }

    pub fn is_owner(&self) -> bool {
        self.shared.feed.lock().is_some()
    }

    /// Close the device and open it again, on the owner thread: what a
    /// restart means here. Respawning the plugin alone left the feed reading
    /// the old process's socket, and the source stayed dark for good.
    pub fn reopen(&self) {
        self.shared.reopen.store(true, Relaxed);
    }
}

impl Drop for Owner {
    /// Stop the thread and wait for it, so the device is let go before a
    /// source with the same id can be made again. Bounded by how long a plugin
    /// may take to start, which the handshake already limits.
    fn drop(&mut self) {
        self.shared.stop.store(true, Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let old = self.shared.feed.lock().take();
        drop(old);
    }
}

fn run_loop(shared: &Arc<Shared>, watch: &Watch) {
    let mut next_try = Instant::now();
    let mut gaps = 0;
    while !shared.stop.load(Relaxed) {
        // Said here rather than on the streaming thread that measured it,
        // which must not wait on a log file.
        if watch.gaps() != gaps {
            gaps = watch.gaps();
            let name = shared.plan.lock().as_ref().map(|p| p.name.to_string()).unwrap_or_default();
            info!(bus = %name, gap_ms = watch.last_gap_ms(), "the picture came back after a gap");
        }
        // Each lock is taken and let go within its own statement: a feed is
        // dropped (which waits for the plugin to stop) with no lock held, so
        // `health` and `call` on the mixer's side never wait on it.
        let mut failed = shared.feed.lock().as_mut().and_then(Feed::failure);
        if shared.reopen.swap(false, Relaxed) && failed.is_none() && shared.feed.lock().is_some() {
            failed = Some("a restart asked for it".into());
        }
        if let Some(why) = failed {
            let name = shared.plan.lock().as_ref().map(|p| p.name.to_string()).unwrap_or_default();
            warn!(bus = %name, %why, "giving the device up so another source can open it");
            let old = shared.feed.lock().take();
            drop(old);
            next_try = Instant::now() + BACKOFF;
        }
        if shared.feed.lock().is_none() && Instant::now() >= next_try {
            if let Err(delay) = try_own(shared) {
                next_try = Instant::now() + delay;
            }
        }
        std::thread::sleep(TICK);
    }
}

/// Take the claim and start the feed if nobody holds it. `Err` carries how
/// long to wait before asking again after a failure.
fn try_own(shared: &Arc<Shared>) -> Result<(), Duration> {
    let Some(plan) = shared.plan.lock().clone() else { return Ok(()) };
    let registry = Registry::new(&plan.dir).map_err(|e| {
        warn!(bus = %plan.name, error = %e, "the frame bus directory is not usable");
        Duration::from_secs(5)
    })?;
    let claim = match Claim::try_take(&registry, &plan.name) {
        Ok(Some(claim)) => claim,
        Ok(None) => return Ok(()),
        Err(e) => {
            warn!(bus = %plan.name, error = %e, "could not ask for the claim");
            return Err(Duration::from_secs(5));
        }
    };
    let began = Instant::now();
    match Feed::start(claim, &plan) {
        Ok(feed) => {
            let ms = began.elapsed().as_millis() as u64;
            if shared.stop.load(Relaxed) {
                return Ok(());
            }
            shared.takeovers.fetch_add(1, Relaxed);
            shared.last_start_ms.store(ms, Relaxed);
            info!(bus = %plan.name, pid = ?feed.pid(), start_ms = ms, "this source opened the device and shares it");
            *shared.feed.lock() = Some(feed);
            Ok(())
        }
        Err(e) => {
            warn!(bus = %plan.name, error = %format!("{e:#}"), "the device would not open; another source may try");
            Err(BACKOFF * 2)
        }
    }
}
