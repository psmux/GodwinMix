//! A show's own CPU, told to its station once a second while the show holds
//! a ticket, so the station's governor counts that work as its own rather
//! than as another program's on top of the ticket (`Governor::set_elsewhere`).
//! A show that holds nothing sends nothing and its thread sleeps.

use super::Line;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

const EVERY: Duration = Duration::from_secs(1);

pub struct Reporter {
    held: AtomicUsize,
    thread: std::thread::Thread,
}

impl Reporter {
    pub fn spawn(out: Sender<String>) -> std::io::Result<Arc<Reporter>> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Arc<Reporter>>(1);
        let handle = std::thread::Builder::new().name("station-link-load".into()).spawn(move || {
            let Ok(me) = rx.recv() else { return };
            me.run(&out);
        })?;
        let me = Arc::new(Reporter { held: AtomicUsize::new(0), thread: handle.thread().clone() });
        let _ = tx.send(me.clone());
        Ok(me)
    }

    pub fn granted(&self) {
        if self.held.fetch_add(1, Ordering::SeqCst) == 0 {
            self.thread.unpark();
        }
    }

    pub fn released(&self) {
        let _ = self.held.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
    }

    fn run(&self, out: &Sender<String>) {
        let mut last: Option<(u64, Instant)> = None;
        loop {
            if self.held.load(Ordering::SeqCst) == 0 {
                last = None;
                std::thread::park();
                continue;
            }
            std::thread::park_timeout(EVERY);
            let Some(now) = godwinmix_govern::load::sys::process_cpu_ns() else { continue };
            let at = Instant::now();
            if let Some((ns, then)) = last {
                let wall = at.duration_since(then).as_nanos().max(1) as f64;
                let millicores = (now.saturating_sub(ns) as f64 / wall * 1000.0).round() as u32;
                let line = Line::call(None, "show.load", json!({ "millicores": millicores }));
                if out.send(line.text()).is_err() {
                    return;
                }
            }
            last = Some((now, at));
        }
    }
}
