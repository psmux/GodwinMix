//! Keeping each show running: one task per show that starts its process,
//! waits for it to exit or to be told to stop, and starts it again when it
//! died, with the plugin host's backoff (three restarts free, then 30
//! seconds doubling) and a limit past which it is left failed with an alert.
//! A show asked to restart itself (`core.restart`) is started again at once
//! and not counted.

use super::child::{self, Start};
use super::state::Station;
use crate::control::methods::lifecycle::RESTART_EXIT_CODE;
use godwinmix_core::state::Severity;
use godwinmix_host::lifecycle::{Backoff, FREE_RESTARTS};
use godwinmix_protocol::shows::ShowState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tracing::{info, warn};

/// Failures in a row, past the free ones, before a show is left failed.
const PAID_RESTARTS: u32 = 3;
/// A show that ran this long before dying starts its count again.
const STABLE: Duration = Duration::from_secs(60);

/// Start supervising `id`. Already supervised is left alone.
pub fn start(st: &Arc<Station>, id: &str) {
    let (tx, rx) = watch::channel(false);
    {
        let mut procs = st.procs.lock();
        let proc = procs.entry(id.to_string()).or_default();
        if proc.stop.is_some() {
            return;
        }
        proc.stop = Some(tx);
        proc.state = ShowState::Starting;
        proc.error = None;
        proc.restarts = 0;
    }
    st.announce(id);
    tokio::spawn(run(st.clone(), id.to_string(), rx));
}

/// Tell `id`'s task to stop it, and wait until it has, for at most the grace
/// a show gets plus a second.
pub async fn stop(st: &Arc<Station>, id: &str) {
    let Some(tx) = st.procs.lock().get_mut(id).and_then(|p| p.stop.take()) else { return };
    let _ = tx.send(true);
    let deadline = Instant::now() + child::STOP_GRACE + Duration::from_secs(1);
    while Instant::now() < deadline {
        if matches!(st.state_of(id), Some(ShowState::Stopped | ShowState::Failed) | None) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn run(st: Arc<Station>, id: String, mut stop: watch::Receiver<bool>) {
    let mut backoff = Backoff::new();
    loop {
        let Some(mut proc) = spawn(&st, &id) else { return };
        let started = Instant::now();
        let status = tokio::select! {
            status = proc.wait() => status,
            _ = stop.changed() => {
                child::stop(&mut proc).await;
                return st.settle(&id, ShowState::Stopped, None);
            }
        };
        if st.stopping.load(Ordering::SeqCst) || *stop.borrow() {
            return st.settle(&id, ShowState::Stopped, None);
        }
        // Shut down on purpose (`core.shutdown`): stopped, not dead. With
        // one show that was the whole mixer stopping, so the station goes too.
        if status.as_ref().is_ok_and(|s| s.success()) {
            info!(show = %id, "show shut down on request");
            st.settle(&id, ShowState::Stopped, None);
            if st.registry.lock().records.len() == 1 {
                st.quit.notify_one();
            }
            return;
        }
        let asked = status.as_ref().ok().and_then(|s| s.code()) == Some(RESTART_EXIT_CODE);
        if started.elapsed() > STABLE {
            backoff.clear();
        }
        if !asked {
            backoff.next_wait();
        }
        let why = match &status {
            Ok(s) => format!("its process ended ({s})"),
            Err(e) => format!("its process could not be waited on ({e})"),
        };
        if backoff.attempts() > FREE_RESTARTS + PAID_RESTARTS {
            let error = format!("{why}, and it kept dying: {} times in a row. Start it again once the cause is fixed.", backoff.attempts());
            st.events.publish_alert(Severity::Error, format!("Show {id} stopped for good: {error}"));
            return st.settle(&id, ShowState::Failed, Some(error));
        }
        let wait = backoff.wait();
        st.died(&id, &why, wait, asked);
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = stop.changed() => return st.settle(&id, ShowState::Stopped, None),
        }
    }
}

/// Start the process, or settle the show as failed with why not.
fn spawn(st: &Arc<Station>, id: &str) -> Option<tokio::process::Child> {
    let secret = godwinmix_core::secrets::random_key(32).unwrap_or_else(|_| format!("{}", std::process::id()));
    let (config, runtime_dir) = {
        let reg = st.registry.lock();
        let record = reg.get(id)?;
        let runtime_dir = (record.id != super::registry::MAIN)
            .then(|| godwinmix_core::config::env_var("RUNTIME_DIR"))
            .flatten()
            .map(|dir| std::path::PathBuf::from(dir).join("shows").join(id));
        (reg.config_of(record), runtime_dir)
    };
    let link = *st.link.get()?;
    let start = Start { id, config: &config, link, secret: &secret, runtime_dir };
    match child::spawn(&st.launch, &start) {
        Ok(proc) => {
            if let Some(p) = st.procs.lock().get_mut(id) {
                p.secret = secret;
                p.pid = proc.id();
                p.state = ShowState::Starting;
            }
            info!(show = id, pid = ?proc.id(), config = %config.display(), "show started");
            Some(proc)
        }
        Err(e) => {
            let error = format!("its process would not start: {e}. Check that {} is still there.", st.launch.exe.display());
            warn!(show = id, %error, "a show would not start");
            st.settle(id, ShowState::Failed, Some(error));
            None
        }
    }
}

impl Station {
    /// The task is done with this show: say where it ended up.
    fn settle(&self, id: &str, state: ShowState, error: Option<String>) {
        if let Some(p) = self.procs.lock().get_mut(id) {
            p.state = state;
            p.error = error;
            p.pid = None;
            p.stop = None;
            p.addr.send_replace(None);
        }
        self.on_air.lock().remove(id);
        self.announce(id);
    }

    fn died(&self, id: &str, why: &str, wait: Duration, asked: bool) {
        if let Some(p) = self.procs.lock().get_mut(id) {
            p.state = ShowState::Starting;
            p.pid = None;
            p.addr.send_replace(None);
            if !asked {
                p.restarts += 1;
                p.error = Some(format!("{why}; the station is starting it again"));
            }
        }
        self.on_air.lock().remove(id);
        if !asked {
            let when = if wait.is_zero() { "now".to_string() } else { format!("in {} seconds", wait.as_secs()) };
            warn!(show = id, why, "a show died and is being started again");
            self.events.publish_alert(Severity::Warning, format!("Show {id} stopped: {why}. Starting it again {when}."));
        }
        self.announce(id);
    }
}
