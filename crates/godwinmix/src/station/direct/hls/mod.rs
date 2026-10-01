//! HLS from a show without compositing: the station's half.
//!
//! The packaging runs in a process of its own, the HLS packager
//! (`station::packager`), so a crash in it costs these outputs and nothing
//! else. The station keeps what must outlive that process: which outputs
//! should run, each one's viewer key, and what each one last reported. It
//! answers `/hls/<output>/...?show=<id>` itself as far as finding the
//! output and letting the player in, then forwards the request to the
//! packager on loopback (`serve.rs`).
//!
//! The packager runs only while at least one HLS output of a direct show is
//! on (`keep.rs`), and is started again when it dies. Meanwhile its outputs
//! report `reconnecting`, or `failed` once it keeps dying, with why.

mod book;
mod child;
pub mod edit;
mod keep;
mod serve;
pub mod spec;
mod wants;

pub use edit::check_sound;
pub use serve::router;

use crate::station::packager::wire::{Report, Want};
use crate::station::state::Station;
use godwinmix_core::hls::Stream;
use godwinmix_protocol::destination::DestinationState;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tracing::warn;

type Key = (String, String);

/// Every HLS output of every direct show, and the packager that runs them.
#[derive(Default)]
pub struct Packagers {
    book: Mutex<Book>,
    wake: tokio::sync::Notify,
    runtime: OnceLock<tokio::runtime::Handle>,
}

#[derive(Default)]
struct Book {
    cards: BTreeMap<Key, Card>,
    /// Moves whenever the cards do, so the packager is handed them again.
    gen: u64,
    reports: BTreeMap<Key, Report>,
    /// Why no packager answers now: the state its outputs show, the
    /// sentence, and since when.
    down: Option<(DestinationState, String, Instant)>,
    /// How many times the packager was started again.
    restarts: u32,
    keeping: bool,
    pid: Option<u32>,
    /// Where the running packager listens, and its secret.
    at: Option<(SocketAddr, String)>,
}

/// One output as the station holds it. The stream carries the id, the
/// params and the viewer key for letting a player in, and no rings.
struct Card {
    want: Want,
    given_key: bool,
    stream: Arc<Stream>,
}

impl Packagers {
    /// The runtime the packager's keeper runs on. Set once, when the
    /// station starts the direct host.
    pub fn use_runtime(&self, handle: tokio::runtime::Handle) {
        let _ = self.runtime.set(handle);
    }

    /// Make the outputs match the shows, and start the packager when there
    /// is something for it to do. Called on the table's thread after every
    /// table, never on a handler. Never waits on the packager.
    pub fn apply(&self, st: &Arc<Station>) {
        let wanted = wants::wants(st);
        let mut book = self.book.lock();
        let before: Vec<Want> = book.cards.values().map(|c| c.want.clone()).collect();
        let mut cards = BTreeMap::new();
        for (key, want) in wanted {
            let old = book.cards.remove(&key);
            let viewer_key = match (&want.spec.viewer_key, &old) {
                (Some(k), _) => k.clone(),
                (None, Some(c)) if !c.given_key => c.want.viewer_key.clone(),
                (None, _) => match godwinmix_core::hls::key::viewer_key(&format!("{}/{}", key.0, key.1)) {
                    Ok(k) => k,
                    Err(e) => {
                        warn!(show = %key.0, output = %key.1, error = %e, "no viewer key for an HLS output");
                        continue;
                    }
                },
            };
            let p = want.spec.params;
            let (show, output) = key.clone();
            let w = Want { show, output, source: want.source, why_not: want.why_not, segment_ms: p.segment_ms, part_ms: p.part_ms, window_s: p.window_s, viewer_key };
            let stream = match old {
                Some(c) if c.stream.params == p && c.stream.viewer_key == w.viewer_key => c.stream,
                _ => Arc::new(Stream::new(&key.1, p, &w.viewer_key)),
            };
            cards.insert(key, Card { want: w, given_key: want.spec.viewer_key.is_some(), stream });
        }
        let moved = !before.iter().eq(cards.values().map(|c| &c.want));
        book.cards = cards;
        if moved {
            book.gen += 1;
        }
        let Book { cards, reports, .. } = &mut *book;
        reports.retain(|k, _| cards.contains_key(k));
        self.keep(st, &mut book);
        self.wake.notify_one();
    }

    fn keep(&self, st: &Arc<Station>, book: &mut Book) {
        if book.cards.is_empty() || book.keeping {
            return;
        }
        let Some(rt) = self.runtime.get() else { return warn!("no runtime for the HLS packager; HLS outputs of direct shows wait") };
        book.keeping = true;
        rt.spawn(keep::keep(st.clone()));
    }
}
