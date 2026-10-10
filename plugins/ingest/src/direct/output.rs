//! One output of a direct show: one thread, reading the hub, sending on.
//!
//! ```text
//!   hub reader (bounded, drops whole GOPs) ──► Sender (restream::run) ──► Link: RTMP, SRT, UDP, RIST, file
//! ```
//!
//! A channel destination is two threads and two queues, because it takes
//! any iterator. Here the hub reader is the queue, and the restreamer's
//! sender pops it straight: one thread, one queue, no copy of a tag (a tag
//! is a pointer to its payload, shared with every other reader). A slow
//! far end fills its own reader's queue, which drops whole GOPs from the
//! front, counted; the input and the other outputs never wait on it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use godwinmix_protocol::destination::{DestinationLive, DestinationState};

use crate::hub::{Hub, Reader, Recv};
use crate::restream::board::Board;
use crate::restream::queue::{Pop, Tags};
use crate::restream::run::{Preamble, Sender};
use crate::restream::Target;
use crate::sends::Wanted;

const LOOK: Duration = Duration::from_millis(250);

pub struct Output {
    pub wanted: Wanted,
    /// The encoder a converting output's video comes from, for its stats.
    pub encoder: Option<String>,
    stop: Arc<AtomicBool>,
    board: Arc<Board>,
}

impl Output {
    /// Start sending. `hub` is the show's hub for a copy, the renditions
    /// hub for an output that converts.
    pub fn start(wanted: Wanted, hub: Hub, encoder: Option<String>) -> Output {
        let (stop, board) = (Arc::new(AtomicBool::new(false)), Arc::new(Board::new()));
        let (w, s, b) = (wanted.clone(), stop.clone(), board.clone());
        let name = format!("gmx-out-{}-{}", wanted.channel, wanted.id);
        let _ = std::thread::Builder::new().name(name).spawn(move || run(&w, &hub, &s, &b));
        Output { wanted, encoder, stop, board }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.board.state(DestinationState::Off);
    }

    /// What it is doing, in the shape a channel destination reports.
    pub fn live(&self) -> DestinationLive {
        let (mut live, _) = self.board.read();
        if self.stop.load(Ordering::Relaxed) {
            live.state = DestinationState::Off;
            live.kbps = 0;
        }
        live
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The output's thread: read, send, and when what it reads ends (a
/// rendition's pair is rebuilt, the input was replaced), read it again.
fn run(w: &Wanted, hub: &Hub, stop: &Arc<AtomicBool>, board: &Arc<Board>) {
    while !stop.load(Ordering::Relaxed) {
        let key = w.reads();
        if key.is_empty() {
            // A rendition the plan has not given its nodes yet.
            board.state(DestinationState::Waiting);
            std::thread::sleep(LOOK);
            continue;
        }
        let tags = Arc::new(Feed { reader: hub.subscribe(&w.app, &key) });
        let sender = Sender {
            target: Target::new(&w.id, &w.platform, &w.url),
            queue: tags,
            board: board.clone(),
            stop: stop.clone(),
            pre: Preamble::default(),
        };
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sender.run())).is_err() {
            board.state(DestinationState::Failed);
            board.error(Some("the output stopped on an internal error; switch it off and on again".into()));
            return;
        }
        if board.read().0.state == DestinationState::Failed {
            // The far end refused the key three times: asking again the
            // same way will not change its mind. It waits to be changed.
            return;
        }
        std::thread::sleep(LOOK);
    }
}

/// A hub reader as the sender's queue.
struct Feed {
    reader: Reader,
}

impl Tags for Feed {
    fn pop(&self, wait: Duration) -> Pop {
        match self.reader.recv_timeout(wait) {
            Recv::Tag(t) => Pop::Tag(t),
            Recv::Ended => Pop::Closed,
            Recv::Timeout => Pop::Empty,
        }
    }

    fn skip_to_latest_keyframe(&self) {
        self.reader.skip_to_latest_keyframe();
    }
}
