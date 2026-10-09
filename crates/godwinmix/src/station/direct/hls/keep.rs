//! Keeping the HLS packager running while it has something to do.
//!
//! ```text
//!   an HLS output on ──► start ──hello──► hand it the outputs ──► read reports, once a second
//!                          ▲                                            │
//!                          │      it exited, hung, or would not start   │
//!                          └──── after 1, 2, 4 ... 30 s ◄───────────────┘
//!   the last HLS output off, or the station stopping ──► stop it, and this task ends
//! ```
//!
//! One task on the station's runtime, started by `Packagers::apply` and
//! ending by itself. It waits on the process and on loopback HTTP with a
//! deadline, and holds the book's lock only to read or write it.

use super::child::{self, Proc};
use crate::station::state::Station;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

const FIRST_WAIT: Duration = Duration::from_secs(1);
const LONGEST_WAIT: Duration = Duration::from_secs(30);
/// A packager that ran this long before it stopped was not crash looping.
const STEADY: Duration = Duration::from_secs(30);
/// Asks in a row it may leave unanswered before it counts as hung. Each ask
/// waits five seconds, so this is about fifteen seconds of silence. One miss
/// used to be enough, and on a loaded machine (a soak test with every core
/// busy) a packager that was slow for five seconds was killed twice in
/// fifteen minutes, which took the watch link off for longer than the
/// stall itself would have: a new packager starts its segments from nothing.
/// A packager that exits is still seen at once, on `wait`.
const MISSES: u32 = 3;

pub async fn keep(st: Arc<Station>) {
    let hls = &st.direct.hls;
    let (mut wait, mut fails) = (FIRST_WAIT, 0u32);
    loop {
        if hls.finished(&st) {
            return;
        }
        let started = Instant::now();
        let ended = match child::start(&st.launch).await {
            Ok(mut proc) => {
                hls.started(&proc);
                let why = serve(&st, &mut proc).await;
                let mut book = hls.book.lock();
                (book.pid, book.at) = (None, None);
                drop(book);
                why
            }
            Err(e) => Some(format!("it would not start ({e:#})")),
        };
        let Some(why) = ended else { continue };
        if started.elapsed() >= STEADY {
            (wait, fails) = (FIRST_WAIT, 0);
        }
        fails += 1;
        warn!(%why, fails, wait_s = wait.as_secs(), "the HLS packager stopped; starting it again");
        hls.down(&why, fails, wait);
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(LONGEST_WAIT);
    }
}

/// Hand a running packager the outputs and read what they do, until it
/// stops (and why) or nothing is wanted of it (None).
async fn serve(st: &Station, proc: &mut Proc) -> Option<String> {
    let hls = &st.direct.hls;
    let mut sent = None;
    let mut misses = 0u32;
    loop {
        if hls.book.lock().cards.is_empty() || st.stopping.load(Ordering::SeqCst) {
            proc.stop().await;
            return None;
        }
        let (gen, wants) = hls.wanted();
        if sent != Some(gen) {
            if let Err(e) = proc.put(&st.http, &wants).await {
                proc.stop().await;
                return Some(format!("it did not take the outputs ({e:#})"));
            }
            sent = Some(gen);
        }
        match proc.reports(&st.http).await {
            Ok(r) => {
                hls.reported(r);
                misses = 0;
            }
            Err(e) => {
                misses += 1;
                if misses >= MISSES {
                    proc.stop().await;
                    return Some(format!("it stopped answering, {misses} asks in a row ({e:#})"));
                }
                debug!(misses, error = %format!("{e:#}"), "the HLS packager did not answer; asking again before calling it hung");
            }
        }
        tokio::select! {
            status = proc.child.wait() => return Some(match status {
                Ok(s) => format!("it exited ({s})"),
                Err(e) => format!("it could not be waited on ({e})"),
            }),
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            _ = hls.wake.notified() => {}
        }
    }
}
