//! One HLS output of a direct show: a thread of its own that reads the
//! show's stream (or the rendition it asked for) off the relay and hands
//! each frame to its session's pipeline.
//!
//! ```text
//!   waiting ──relay and stream known──► connecting ──first segment──► live
//!                                           ▲                          │
//!                                           └──── lost, after 1 s ◄────┘
//!   a sound codec MP4 does not carry ──► failed, looked at again every 5 s
//! ```
//!
//! The thread is the only thing that waits: on its socket, and on a stop
//! flag between reads. Nothing here runs on a GStreamer streaming thread or
//! a bus handler. A packager that fails costs its own output only, and one
//! that takes the whole process down costs the HLS outputs of direct shows
//! and nothing else: the station starts the process again.

use super::board::Board;
use super::feed::session;
use super::flv;
use super::wire::Source;
use godwinmix_core::hls::Stream;
use godwinmix_protocol::destination::DestinationState as S;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How long one read waits before the stop flag is looked at.
const READ: Duration = Duration::from_secs(1);

pub struct Packager {
    pub stream: Arc<Stream>,
    pub board: Arc<Board>,
    stop: Arc<AtomicBool>,
}

pub enum End {
    Stopped,
    Lost(String),
    /// What the output cannot carry, in a sentence.
    Refused(String),
    /// New caps: build again at once.
    Again,
}

impl Packager {
    /// Start one. With no source yet it runs nothing, waits, and says why.
    pub fn start(stream: Arc<Stream>, source: Option<Source>, why_not: Option<String>, sound: impl Fn(&str) -> String + Send + 'static) -> Packager {
        let (stop, board) = (Arc::new(AtomicBool::new(false)), Arc::new(Board::default()));
        let Some(src) = source else {
            board.set(S::Waiting, why_not);
            return Packager { stream, board, stop };
        };
        let (st, b, s) = (stream.clone(), board.clone(), stop.clone());
        let name = format!("gmx-hls-{}", stream.id);
        let ran = std::thread::Builder::new().name(name).spawn(move || run(&src, &st, &b, &s, &sound));
        if let Err(e) = ran {
            board.set(S::Failed, Some(format!("no thread for the packager ({e}); remove the output and add it again")));
        }
        Packager { stream, board, stop }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Packager {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(src: &Source, stream: &Arc<Stream>, board: &Board, stop: &AtomicBool, sound: &dyn Fn(&str) -> String) {
    let mut ever = false;
    while !stop.load(Ordering::Relaxed) {
        if board.state() != S::Failed {
            board.set(if ever { S::Reconnecting } else { S::Connecting }, board.read().error);
        }
        let end = match flv::Reader::open(src.relay, &src.path, READ) {
            Ok(reader) => {
                if ever {
                    board.reconnected();
                }
                ever = true;
                session(reader, stream, board, stop, sound)
            }
            Err(e) => End::Lost(format!("the relay at {} did not answer ({e})", src.relay)),
        };
        let wait = match end {
            End::Stopped => return,
            End::Again => continue,
            End::Lost(why) => {
                board.set(S::Reconnecting, Some(why));
                Duration::from_secs(1)
            }
            End::Refused(why) => {
                board.set(S::Failed, Some(why));
                Duration::from_secs(5)
            }
        };
        let until = std::time::Instant::now() + wait;
        while !stop.load(Ordering::Relaxed) && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
