//! Sending a channel's stream straight on to YouTube, Facebook, Twitch, any
//! RTMP or RTMPS server, or SRT. The publisher's own bytes are remuxed: no
//! decode, no encode.
//!
//! ```text
//!   tags ──► pump thread ──► Queue (bounded, drops whole GOPs) ──► sender thread ──► Link
//! ```
//!
//! Each destination is two threads of its own. The pump only moves tags from
//! whatever feeds it into the queue and never waits on the far end, so a slow
//! or dead destination costs its own GOPs and nothing else: not the
//! publisher, not the other destinations. The sender owns the connection,
//! dials again on the backoff the core's outputs use, and gives the far end
//! the metadata and sequence headers first on every connection, then nothing
//! until a keyframe.
//!
//! [`start`] takes any iterator of [`MediaTag`], and a `Receiver<MediaTag>`
//! is one, so the channel hub's reader plugs straight in.

mod board;
mod io;
mod link;
pub(crate) mod meta;
mod queue;
mod rtmp_out;
mod run;
mod srt_out;
pub(crate) mod ts_video;
mod target;

#[cfg(test)]
mod fanout;
#[cfg(test)]
pub mod test_gate;
#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use godwinmix_protocol::destination::{DestinationLive, DestinationState};

use crate::media_tag::MediaTag;

pub use queue::Dropped;
pub use target::Target;

/// What one destination is doing, for `Destination` on the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    pub live: DestinationLive,
    /// Tags and GOPs lost because the far end was slower than the stream.
    pub dropped: Dropped,
    /// Bytes sent since it started.
    pub bytes: u64,
}

/// A running destination. Dropping it switches the destination off.
pub struct Handle {
    stop: Arc<AtomicBool>,
    queue: Arc<queue::Queue>,
    board: Arc<board::Board>,
}

/// Start sending `tags` to `target`. Returns at once; the connecting happens
/// on the destination's own thread.
pub fn start<I>(target: Target, tags: I) -> Handle
where
    I: IntoIterator<Item = MediaTag>,
    I::IntoIter: Send + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    let queue = Arc::new(queue::Queue::new(target.queue_bytes));
    let board = Arc::new(board::Board::new());
    let name = target.id.clone();

    let (q, s) = (Arc::clone(&queue), Arc::clone(&stop));
    let tags = tags.into_iter();
    let _ = std::thread::Builder::new().name(format!("gmx-restream-in-{name}")).spawn(move || {
        for tag in tags {
            if s.load(Ordering::Relaxed) {
                break;
            }
            q.push(tag);
        }
        q.close();
    });

    let sender = run::Sender {
        target,
        queue: Arc::clone(&queue),
        board: Arc::clone(&board),
        stop: Arc::clone(&stop),
        pre: run::Preamble::default(),
    };
    let on_panic = Arc::clone(&board);
    let _ = std::thread::Builder::new().name(format!("gmx-restream-{name}")).spawn(move || {
        // A bug in here must not leave the row saying "connecting" forever.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sender.run())).is_err() {
            on_panic.state(DestinationState::Failed);
            on_panic.error(Some("the restreamer stopped on an internal error; switch the \
                                 destination off and on again".into()));
        }
    });
    Handle { stop, queue, board }
}

impl Handle {
    pub fn stats(&self) -> Stats {
        let (mut live, bytes) = self.board.read();
        if self.stop.load(Ordering::Relaxed) {
            live.state = DestinationState::Off;
            live.kbps = 0;
        }
        Stats { live, dropped: self.queue.dropped(), bytes }
    }

    /// Switch the destination off. The sender says goodbye to the far end and
    /// ends on its own thread; nothing here waits for the network.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.board.state(DestinationState::Off);
        self.queue.close();
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stop();
    }
}
