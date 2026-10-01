//! Keeping each show running: one task per show that starts its process,
//! waits for it to exit or to be told to stop, and starts it again when it
//! died, with the plugin host's backoff (three restarts free, then 30
//! seconds doubling) and a limit past which it is left failed with an alert.
//! A show asked to restart itself (`core.restart`) is started again at once
//! and not counted. The rules are in `decide.rs`.

mod decide;
mod report;

use super::child::{self, Start};
use super::state::Station;
use crate::control::methods::lifecycle::RESTART_EXIT_CODE;
use decide::{Exit, Next, Tally};
use godwinmix_core::state::Severity;
use godwinmix_protocol::shows::ShowState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tracing::{info, warn};

/// Start supervising `id`. Already supervised is left alone.
pub fn start(st: &Arc<Station>, id: &str) {
    if st.is_direct(id) {
        return;
    }
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
    st.direct.starting(id);
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
    let mut tally = Tally::default();
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
        let why = match &status {
            Ok(s) => format!("its process ended ({s})"),
            Err(e) => format!("its process could not be waited on ({e})"),
        };
        match tally.after(Exit::of(&status, RESTART_EXIT_CODE), started.elapsed()) {
            Next::Stop => return st.shut_down(&id),
            Next::Fail { times } => {
                let error = format!("{why}, and it kept dying: {times} times in a row. Start it again once the cause is fixed.");
                st.events.publish_alert(Severity::Error, format!("Show {id} stopped for good: {error}"));
                return st.settle(&id, ShowState::Failed, Some(error));
            }
            Next::Again { wait, counted } => {
                st.died(&id, &why, wait, counted);
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = stop.changed() => return st.settle(&id, ShowState::Stopped, None),
                }
            }
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
