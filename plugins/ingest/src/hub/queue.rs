//! One reader's bounded queue.
//!
//! The publisher's connection thread puts tags in; the reader's own thread
//! takes them out. When a reader falls behind, the queue does not grow and
//! the publisher does not wait: whole GOPs are dropped from the front, the
//! reader starts again at the next keyframe, and the loss is counted.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::media_tag::{MediaTag, TagKind};

/// How much a reader may have waiting before it loses a GOP. About five
/// seconds of a 6 Mbit/s stream, or a thousand tags of anything.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TAGS: usize = 1024;

/// The codec headers and metadata a reader needs before anything else.
#[derive(Default, Clone)]
pub struct Headers {
    pub script: Option<MediaTag>,
    pub video: Option<MediaTag>,
    pub audio: Option<MediaTag>,
}

impl Headers {
    fn in_order(&self) -> impl Iterator<Item = &MediaTag> {
        [&self.script, &self.video, &self.audio].into_iter().flatten()
    }
}

#[derive(Default)]
struct Queue {
    tags: VecDeque<MediaTag>,
    bytes: usize,
    /// Waiting for a keyframe before taking anything more.
    skipping: bool,
    /// A header was not delivered, so the headers go first when it resumes.
    owe_headers: bool,
    dropped_gops: u64,
    ended: bool,
}

/// What `recv` found.
#[derive(Debug)]
pub enum Recv {
    Tag(MediaTag),
    /// The publisher left. Nothing more will come on this queue.
    Ended,
    Timeout,
}

/// The queue and the signal that something is in it.
pub struct Shared {
    queue: Mutex<Queue>,
    ready: Condvar,
}

impl Shared {
    /// A queue that starts at the next keyframe with the headers in front.
    pub fn new() -> Shared {
        let queue = Queue { skipping: true, owe_headers: true, ..Queue::default() };
        Shared { queue: Mutex::new(queue), ready: Condvar::new() }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Offer one tag. Never blocks on the reader. `headers` are the ones in
    /// force, already updated with this tag when it is one.
    pub fn offer(&self, tag: &MediaTag, headers: &Headers, audio_only: bool) {
        let mut q = self.lock();
        if q.ended {
            return;
        }
        let header = tag.sequence_header || tag.kind == TagKind::Script;
        if q.skipping {
            let starts = if audio_only { tag.kind == TagKind::Audio } else { is_gop_start(tag) };
            if header || !starts {
                q.owe_headers |= header;
                return;
            }
            q.skipping = false;
        }
        if std::mem::take(&mut q.owe_headers) {
            for h in headers.in_order() {
                if !(header && h.kind == tag.kind) {
                    q.push(h.clone());
                }
            }
        }
        q.push(tag.clone());
        while q.bytes > MAX_BYTES || q.tags.len() > MAX_TAGS {
            q.drop_front_gop();
        }
        if q.owe_headers && !q.skipping {
            // A header went with the dropped GOP and a keyframe is now at the
            // front: the headers go in front of it rather than after it.
            q.owe_headers = false;
            for h in headers.in_order().collect::<Vec<_>>().into_iter().rev() {
                q.bytes += h.payload.len();
                q.tags.push_front(h.clone());
            }
        }
        drop(q);
        self.ready.notify_one();
    }

    /// The publisher has gone.
    pub fn end(&self) {
        self.lock().ended = true;
        self.ready.notify_all();
    }

    /// Take the next tag, waiting at most `wait`.
    pub fn recv(&self, wait: Duration) -> Recv {
        let mut q = self.lock();
        loop {
            if let Some(tag) = q.tags.pop_front() {
                q.bytes -= tag.payload.len();
                return Recv::Tag(tag);
            }
            if q.ended {
                return Recv::Ended;
            }
            let (next, timeout) =
                self.ready.wait_timeout(q, wait).unwrap_or_else(|e| e.into_inner());
            q = next;
            if timeout.timed_out() && q.tags.is_empty() && !q.ended {
                return Recv::Timeout;
            }
        }
    }

    pub fn dropped_gops(&self) -> u64 {
        self.lock().dropped_gops
    }

    #[cfg(test)]
    pub fn waiting(&self) -> (usize, usize) {
        let q = self.lock();
        (q.tags.len(), q.bytes)
    }
}

fn is_gop_start(tag: &MediaTag) -> bool {
    tag.kind == TagKind::Video && tag.keyframe && !tag.sequence_header
}

impl Queue {
    fn push(&mut self, tag: MediaTag) {
        self.bytes += tag.payload.len();
        self.tags.push_back(tag);
    }

    /// Drop the GOP at the front: everything up to the next keyframe. With no
    /// keyframe left in the queue, drop it all and wait for the next one.
    fn drop_front_gop(&mut self) {
        let mut first = true;
        while let Some(front) = self.tags.front() {
            if !first && is_gop_start(front) {
                break;
            }
            first = false;
            let gone = self.tags.pop_front().expect("front was there");
            self.bytes -= gone.payload.len();
            self.owe_headers |= gone.sequence_header || gone.kind == TagKind::Script;
        }
        if self.tags.is_empty() {
            self.skipping = true;
        }
        self.dropped_gops += 1;
    }
}
