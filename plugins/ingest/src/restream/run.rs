//! One destination's sender thread: wait for the stream, dial, send, and
//! when the far end goes, dial again on the backoff.
//!
//! ```text
//!   waiting ──first tag──► connecting ──yes──► live ──lost──► reconnecting ──yes──► live
//!                              │                                  │
//!                              └──key refused──► failed ◄─────────┘
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use godwinmix_protocol::destination::DestinationState as S;

use crate::media_tag::{MediaTag, TagKind};

use super::board::Board;
use super::link::{self, Failure, Link};
use super::queue::{starts_gop, Pop, Queue};
use super::target::Target;

/// How often a quiet link reads what the server said.
const POLL: Duration = Duration::from_millis(250);
/// Refusals in a row before the sender stops asking.
const GIVE_UP: u32 = 3;

pub struct Sender {
    pub target: Target,
    pub queue: Arc<Queue>,
    pub board: Arc<Board>,
    pub stop: Arc<AtomicBool>,
    pub pre: Preamble,
}

/// What a far end is given first on every connection, newest of each.
#[derive(Default)]
pub struct Preamble {
    meta: Option<MediaTag>,
    video: Option<MediaTag>,
    audio: Option<MediaTag>,
}

impl Preamble {
    /// Keep a header tag. Answers false for a media tag.
    fn remember(&mut self, tag: &MediaTag) -> bool {
        let slot = match (tag.kind, tag.sequence_header) {
            (TagKind::Script, _) => &mut self.meta,
            (TagKind::Video, true) => &mut self.video,
            (TagKind::Audio, true) => &mut self.audio,
            _ => return false,
        };
        *slot = Some(tag.clone());
        true
    }

    fn tags(&self) -> impl Iterator<Item = &MediaTag> {
        [&self.meta, &self.video, &self.audio].into_iter().flatten()
    }
}

enum End {
    Stopped,
    Closed,
    Lost(Failure),
}

impl Sender {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn run(mut self) {
        self.board.state(S::Waiting);
        let mut pending = None;
        while pending.is_none() {
            match self.queue.pop(Duration::from_millis(100)) {
                _ if self.stopped() => return,
                Pop::Tag(t) if !self.pre.remember(&t) => pending = Some(t),
                Pop::Tag(_) | Pop::Empty => {}
                Pop::Closed => return,
            }
        }
        self.dial_loop(pending);
    }

    fn dial_loop(&mut self, mut pending: Option<MediaTag>) {
        let retry = self.target.policy.retry();
        let (mut attempt, mut refusals, mut ever) = (0u32, 0u32, false);
        while !self.stopped() {
            self.board.state(if ever { S::Reconnecting } else { S::Connecting });
            let failure = match link::dial(&self.target) {
                Ok(link) => {
                    if ever {
                        self.board.reconnected();
                    }
                    (ever, attempt, refusals) = (true, 0, 0);
                    match self.pump(link, pending.take()) {
                        End::Stopped => return,
                        End::Closed => return self.board.state(S::Waiting),
                        End::Lost(f) => f,
                    }
                }
                Err(f) => f,
            };
            self.board.error(Some(failure.message().to_string()));
            let mut wait = retry.delay_for(attempt);
            if let Failure::Refused(_) = failure {
                self.board.state(S::Failed);
                refusals += 1;
                if refusals >= GIVE_UP {
                    return self.queue.close();
                }
                wait = Duration::from_millis(retry.max_delay_ms);
            }
            attempt += 1;
            self.sleep(wait);
            self.queue.skip_to_latest_keyframe();
        }
    }

    fn sleep(&self, total: Duration) {
        let until = Instant::now() + total;
        while !self.stopped() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50).min(until - Instant::now()));
        }
    }

    /// Send until the link fails, the stream ends or the destination is
    /// switched off. The headers go first, then nothing until a keyframe.
    fn pump(&mut self, mut link: Box<dyn Link>, mut pending: Option<MediaTag>) -> End {
        self.board.state(S::Live);
        self.board.error(None);
        for tag in self.pre.tags() {
            if let Err(f) = link.send(tag, 0) {
                return End::Lost(f);
            }
        }
        let mut need_key = self.pre.video.is_some();
        let (mut base, mut polled) = (None, Instant::now());
        loop {
            if self.stopped() {
                link.close();
                return End::Stopped;
            }
            let tag = match pending.take() {
                Some(t) => Some(t),
                None => match self.queue.pop(POLL) {
                    Pop::Tag(t) => Some(t),
                    Pop::Empty => None,
                    Pop::Closed => {
                        link.close();
                        return End::Closed;
                    }
                },
            };
            if tag.is_none() || polled.elapsed() >= POLL {
                polled = Instant::now();
                if let Err(f) = link.poll() {
                    return End::Lost(f);
                }
            }
            let Some(tag) = tag else { continue };
            let header = self.pre.remember(&tag);
            if !header && need_key {
                if tag.kind == TagKind::Audio || !starts_gop(&tag) {
                    continue;
                }
                need_key = false;
            }
            let at = *base.get_or_insert(tag.timestamp_ms);
            match link.send(&tag, tag.timestamp_ms.saturating_sub(at)) {
                Ok(n) => self.board.sent(n),
                Err(f) => return End::Lost(f),
            }
        }
    }
}
