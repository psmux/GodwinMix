//! The terminal copy of a line logged on the mixer thread, written by a
//! thread of its own.
//!
//! Every log line goes to `godwinmix.log` and to stderr. Under the desktop
//! app stderr is a pipe the app reads, and in the seconds after a restart
//! the app is busy opening its window and reads it late. A line written to a
//! full pipe waits, and the mixer thread that serves Take was the one
//! waiting: after a restart, supervisor ticks held the loop for 0.5 to 3.4
//! seconds in steps whose only slow part was writing a log line.
//!
//! So a line logged on the mixer thread is queued for a writer thread, and
//! a full queue drops the stderr copy and counts it. The file copy is
//! written as before, so nothing is lost from `godwinmix.log`. Lines from
//! every other thread are written straight away, as before, so the reason a
//! mixer exits still reaches the app before it does.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::Arc;

/// Lines that may wait for the writer before new ones are dropped.
const BACKLOG: usize = 2048;

/// Where a line goes: `write` for the thread named `queued_for`, through
/// the queue; `write` directly for everyone else.
pub struct Queued {
    tx: SyncSender<String>,
    dropped: Arc<AtomicU64>,
    queued_for: &'static str,
    write: Arc<dyn Fn(&str) + Send + Sync>,
}

impl Queued {
    /// Start the writer. `notice` turns a count of dropped lines into the
    /// line that says so, in the log's own format.
    pub fn start(
        queued_for: &'static str,
        write: Arc<dyn Fn(&str) + Send + Sync>,
        notice: fn(u64) -> String,
    ) -> std::io::Result<Queued> {
        let (tx, rx) = sync_channel::<String>(BACKLOG);
        let dropped = Arc::new(AtomicU64::new(0));
        let (seen, out) = (dropped.clone(), write.clone());
        std::thread::Builder::new().name("log-stderr".into()).spawn(move || {
            let mut said = 0;
            for line in rx {
                let lost = seen.load(Ordering::Relaxed);
                if lost > said {
                    out(&notice(lost - said));
                    said = lost;
                }
                out(&line);
            }
        })?;
        Ok(Queued { tx, dropped, queued_for, write })
    }

    pub fn line(&self, line: &str) {
        if std::thread::current().name() != Some(self.queued_for) {
            return (self.write)(line);
        }
        if let Err(TrySendError::Full(_)) = self.tx.try_send(line.to_string()) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// A writer stuck on a full pipe holds up its own thread, not the one
    /// logging; what did not fit is counted and said once it moves again.
    #[test]
    fn a_stuck_writer_does_not_hold_the_thread_that_logs() {
        let gate = Arc::new(Mutex::new(()));
        let held = gate.lock().unwrap();
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let (g, l) = (gate.clone(), lines.clone());
        let write: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |s: &str| {
            let _wait = g.lock().unwrap();
            l.lock().unwrap().push(s.to_string());
        });
        let q = Queued::start("queued-test", write, |n| format!("lost {n}")).unwrap();
        let t = std::thread::Builder::new()
            .name("queued-test".into())
            .spawn(move || {
                let started = Instant::now();
                for n in 0..BACKLOG + 100 {
                    q.line(&format!("line {n}"));
                }
                (started.elapsed(), q)
            })
            .unwrap();
        let (took, q) = t.join().unwrap();
        assert!(took < Duration::from_secs(1), "logging waited on the writer: {took:?}");
        drop(held);
        q.line("after");
        drop(q);
        let deadline = Instant::now() + Duration::from_secs(5);
        let said = |l: &Vec<String>| l.iter().any(|s| s.starts_with("lost ")) && l.iter().any(|s| s == "after");
        while !said(&lines.lock().unwrap()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let lines = lines.lock().unwrap();
        assert!(lines.iter().any(|s| s.starts_with("lost ")), "the dropped lines were never said");
        assert!(lines.iter().any(|s| s == "after"), "a line from another thread went missing");
    }
}
