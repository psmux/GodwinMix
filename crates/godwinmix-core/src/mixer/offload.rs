//! Slow work for one source or output, done on a thread of its own so the
//! mixer thread carries on answering.
//!
//! Restarting a source takes its pipeline to NULL and back, and NULL joins
//! every streaming thread in it. Stopping one does the same and then stops
//! whatever the kind holds outside the pipeline, a plugin process or a device.
//! Rebuilding an output does both to the output's pipeline. Each of those can
//! take seconds on a bad day and forever on a worse one: on 2026-10-01 a stall
//! restart of `cam-pulpit` held the command loop for more than five minutes,
//! and every call in that time, a status read or a take, answered that the
//! loop was "held by source.restart". The first rule of the core is that one
//! source going wrong costs that source and nothing else, and a take that
//! waits on a camera nobody is looking at breaks it.
//!
//! So the work goes here. The mixer marks the thing as busy, carries on, and
//! the thread sends a command back when it is done. A worker that overruns
//! gets a line in the log naming it, and another when it finally finishes; one
//! that never finishes costs a parked thread, not the mixer.

use std::sync::mpsc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// How long a worker may take before the log says so. A restart that is
/// working takes a few hundred milliseconds.
pub const OVERRUN: Duration = Duration::from_secs(3);

/// Run `work` on a thread named after what it is and whose it is.
///
/// Returns false when no thread could be started, in which case `work` never
/// ran and the caller undoes whatever it marked.
pub fn run<F>(what: &'static str, id: &str, work: F) -> bool
where
    F: FnOnce() + Send + 'static,
{
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let owner = id.to_string();
    let spawned = std::thread::Builder::new()
        .name(format!("{what}-{id}"))
        .spawn(move || {
            let started = Instant::now();
            work();
            let _ = done_tx.send(());
            let took = started.elapsed();
            if took > OVERRUN {
                info!(id = %owner, what, took_ms = took.as_millis() as u64, "slow work finished off the mixer thread");
            }
        });
    if let Err(e) = spawned {
        warn!(id, what, ?e, "could not start a thread for work that must not run on the mixer thread");
        return false;
    }
    watch(what, id.to_string(), done_rx);
    true
}

/// One line if the worker overruns. A thread of its own because the worker
/// cannot speak while it is stuck, and it ends at the first of the two.
fn watch(what: &'static str, id: String, done: mpsc::Receiver<()>) {
    let _ = std::thread::Builder::new().name(format!("watch-{what}")).spawn(move || {
        if let Err(mpsc::RecvTimeoutError::Timeout) = done.recv_timeout(OVERRUN) {
            warn!(
                id = %id,
                what,
                after_ms = OVERRUN.as_millis() as u64,
                "slow work is still running off the mixer thread; the mixer carries on without it"
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caller_does_not_wait_for_the_work() {
        let (tx, rx) = mpsc::channel();
        let started = Instant::now();
        assert!(run("test", "slow", move || {
            std::thread::sleep(Duration::from_millis(300));
            let _ = tx.send(());
        }));
        assert!(started.elapsed() < Duration::from_millis(100));
        rx.recv_timeout(Duration::from_secs(5)).expect("the work ran");
    }
}
