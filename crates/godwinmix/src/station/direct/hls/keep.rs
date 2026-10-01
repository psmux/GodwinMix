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
use super::{Book, Packagers};
use crate::station::packager::wire::{Report, Want};
use crate::station::state::Station;
use godwinmix_protocol::destination::DestinationState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

const FIRST_WAIT: Duration = Duration::from_secs(1);
const LONGEST_WAIT: Duration = Duration::from_secs(30);
/// A packager that ran this long before it stopped was not crash looping.
const STEADY: Duration = Duration::from_secs(30);
/// Stops in a row before its outputs say `failed` rather than `reconnecting`.
const FAILED_AFTER: u32 = 5;

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
            Ok(r) => hls.reported(r),
            Err(e) => {
                proc.stop().await;
                return Some(format!("it stopped answering ({e:#})"));
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

impl Packagers {
    /// Whether the keeper should end: nothing wanted, or the station is
    /// stopping. Decided under the lock `apply` takes, so an output added
    /// meanwhile starts a new keeper rather than finding this one gone.
    fn finished(&self, st: &Station) -> bool {
        let mut book = self.book.lock();
        if !book.cards.is_empty() && !st.stopping.load(Ordering::SeqCst) {
            return false;
        }
        book.keeping = false;
        (book.down, book.pid, book.at) = (None, None, None);
        book.reports.clear();
        true
    }

    fn started(&self, proc: &Proc) {
        info!(addr = %proc.addr, "the HLS packager is up");
        let mut book = self.book.lock();
        if book.down.is_some() {
            book.restarts += 1;
        }
        book.pid = proc.child.id();
        book.at = Some((proc.addr, proc.secret.clone()));
        book.down = None;
    }

    fn wanted(&self) -> (u64, Vec<Want>) {
        let book = self.book.lock();
        (book.gen, book.cards.values().map(|c| c.want.clone()).collect())
    }

    fn reported(&self, reports: Vec<Report>) {
        let mut book = self.book.lock();
        let Book { cards, reports: kept, .. } = &mut *book;
        *kept = reports.into_iter().map(|r| ((r.show.clone(), r.output.clone()), r)).filter(|(k, _)| cards.contains_key(k)).collect();
    }

    /// The packager is gone: say so on every output, with what happens next.
    fn down(&self, why: &str, fails: u32, wait: Duration) {
        let s = wait.as_secs();
        let (state, text) = if fails < FAILED_AFTER {
            (
                DestinationState::Reconnecting,
                format!("the HLS packager stopped: {why}. The station starts it again in {s} s; the link stays the same, and a player may need to open it again once it is back."),
            )
        } else {
            (
                DestinationState::Failed,
                format!("the HLS packager has stopped {fails} times in a row, last: {why}. The station still starts it again every {s} s; the station's log, the lines from node hls-packager, says why it stops."),
            )
        };
        let mut book = self.book.lock();
        let since = match &book.down {
            Some((was, _, at)) if *was == state => *at,
            _ => Instant::now(),
        };
        book.down = Some((state, text, since));
        book.reports.clear();
    }
}
