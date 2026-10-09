//! What the station knows of its HLS outputs: read by `show.list`,
//! `show.stats` and the `/hls` door, and written by the packager's keeper.
//! Every lock here is held for a lookup or an assignment.

use super::child::Proc;
use super::{Book, Packagers};
use crate::station::packager::wire::{Report, Want};
use crate::station::state::Station;
use godwinmix_core::hls::Stream;
use godwinmix_protocol::destination::{DestinationLive, DestinationState, Playback};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

/// Stops in a row before its outputs say `failed` rather than `reconnecting`.
const FAILED_AFTER: u32 = 5;

impl Packagers {
    /// The card for one output, for letting a player in.
    pub fn stream(&self, show: &str, output: &str) -> Option<Arc<Stream>> {
        self.book.lock().cards.get(&(show.to_string(), output.to_string())).map(|c| c.stream.clone())
    }

    /// The HLS outputs one show serves now.
    pub fn ids(&self, show: &str) -> Vec<String> {
        self.book.lock().cards.keys().filter(|(s, _)| s == show).map(|(_, o)| o.clone()).collect()
    }

    /// Where the running packager listens, and the secret it takes.
    pub fn packager(&self) -> Option<(SocketAddr, String)> {
        self.book.lock().at.clone()
    }

    /// The packager's process, for what the station's children cost.
    pub fn pid(&self) -> Option<u32> {
        self.book.lock().pid
    }

    /// What one output is doing, and where a player opens it.
    pub fn view(&self, show: &str, output: &str) -> Option<(DestinationLive, Playback)> {
        let book = self.book.lock();
        let key = (show.to_string(), output.to_string());
        let card = book.cards.get(&key)?;
        let report = book.reports.get(&key);
        let live = match (&book.down, report) {
            (Some((state, why, at)), _) => {
                DestinationLive { state: *state, since_ms: at.elapsed().as_millis() as u64, reconnects: book.restarts, error: Some(why.clone()), kbps: 0, file: None }
            }
            (None, Some(r)) => DestinationLive { reconnects: r.live.reconnects + book.restarts, ..r.live.clone() },
            (None, None) => {
                let error = Some("handing the output to the HLS packager".to_string());
                DestinationLive { state: DestinationState::Waiting, reconnects: book.restarts, error, ..Default::default() }
            }
        };
        let q = format!("show={show}&key={}", card.want.viewer_key);
        let playback = Playback {
            master_url_path: format!("/hls/{output}/master.m3u8?{q}"),
            dash_url_path: format!("/hls/{output}/manifest.mpd?{q}"),
            viewers: report.map(|r| r.viewers).unwrap_or(0),
        };
        Some((live, playback))
    }

    /// Whether the keeper should end: nothing wanted, or the station is
    /// stopping. Decided under the lock `apply` takes, so an output added
    /// meanwhile starts a new keeper rather than finding this one gone.
    pub(super) fn finished(&self, st: &Station) -> bool {
        let mut book = self.book.lock();
        if !book.cards.is_empty() && !st.stopping.load(Ordering::SeqCst) {
            return false;
        }
        book.keeping = false;
        (book.down, book.pid, book.at) = (None, None, None);
        book.reports.clear();
        true
    }

    pub(super) fn started(&self, proc: &Proc) {
        info!(addr = %proc.addr, "the HLS packager is up");
        let mut book = self.book.lock();
        if book.down.is_some() {
            book.restarts += 1;
        }
        book.pid = proc.child.id();
        book.at = Some((proc.addr, proc.secret.clone()));
        book.down = None;
    }

    pub(super) fn wanted(&self) -> (u64, Vec<Want>) {
        let book = self.book.lock();
        (book.gen, book.cards.values().map(|c| c.want.clone()).collect())
    }

    pub(super) fn reported(&self, reports: Vec<Report>) {
        let mut book = self.book.lock();
        let Book { cards, reports: kept, .. } = &mut *book;
        *kept = reports.into_iter().map(|r| ((r.show.clone(), r.output.clone()), r)).filter(|(k, _)| cards.contains_key(k)).collect();
    }

    /// The packager is gone: say so on every output, with what happens next.
    pub(super) fn down(&self, why: &str, fails: u32, wait: Duration) {
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
