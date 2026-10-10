//! The bounded queue between a stream and one destination.
//!
//! The publisher's side pushes and never waits. When the destination falls
//! behind and the queue passes its size, whole GOPs go from the front, so what
//! is left still starts at a keyframe; with no keyframe left to stop at, the
//! queue empties and takes nothing more until the next one arrives. Sequence
//! headers and metadata are never dropped: a decoder needs them whatever else
//! it missed. Every loss is counted.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::media_tag::{MediaTag, TagKind};

/// What a pop found.
#[derive(Debug)]
pub enum Pop {
    Tag(MediaTag),
    /// Nothing arrived in the time given.
    Empty,
    /// The stream ended and everything in the queue has been taken.
    Closed,
}

/// Losses, for the stats a destination reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Dropped {
    pub tags: u64,
    pub gops: u64,
}

/// What a sender reads its tags from: this queue for a channel destination,
/// a hub reader straight for a direct show's output, which saves a thread
/// and a queue per output.
pub trait Tags: Send + Sync {
    fn pop(&self, wait: Duration) -> Pop;
    /// Throw away what waits, back to the newest keyframe, headers apart.
    fn skip_to_latest_keyframe(&self);
}

impl Tags for Queue {
    fn pop(&self, wait: Duration) -> Pop {
        Queue::pop(self, wait)
    }

    fn skip_to_latest_keyframe(&self) {
        Queue::skip_to_latest_keyframe(self)
    }
}

pub struct Queue {
    inner: Mutex<Inner>,
    ready: Condvar,
    cap_bytes: usize,
}

#[derive(Default)]
struct Inner {
    tags: VecDeque<MediaTag>,
    bytes: usize,
    closed: bool,
    /// Past an overflow with no keyframe to stop at: take nothing until one.
    skipping: bool,
    dropped: Dropped,
}

/// Always kept, whatever is dropped around it.
fn is_header(tag: &MediaTag) -> bool {
    tag.sequence_header || tag.kind == TagKind::Script
}

/// A video tag a decoder can start from.
pub fn starts_gop(tag: &MediaTag) -> bool {
    tag.kind == TagKind::Video && tag.keyframe && !tag.sequence_header
}

impl Queue {
    pub fn new(cap_bytes: usize) -> Queue {
        Queue { inner: Mutex::new(Inner::default()), ready: Condvar::new(), cap_bytes }
    }

    /// Take a tag. Never blocks on the destination.
    pub fn push(&self, tag: MediaTag) {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if q.closed {
            // The sender has gone: nothing will read this.
            return;
        }
        if q.skipping && !is_header(&tag) {
            if !starts_gop(&tag) {
                q.dropped.tags += 1;
                return;
            }
            q.skipping = false;
        }
        q.bytes += tag.payload.len();
        q.tags.push_back(tag);
        while q.bytes > self.cap_bytes {
            // The GOP at the front goes, up to the keyframe after its first
            // media tag. Headers in front of it do not count as a start.
            let Some(first) = q.tags.iter().position(|t| !is_header(t)) else { break };
            let next = q.tags.iter().skip(first + 1).position(starts_gop).map(|i| i + first + 1);
            drop_front(&mut q, next);
            q.dropped.gops += 1;
        }
        drop(q);
        self.ready.notify_one();
    }

    /// Wait up to `wait` for the next tag.
    pub fn pop(&self, wait: Duration) -> Pop {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if q.tags.is_empty() && !q.closed {
            q = self.ready.wait_timeout(q, wait).unwrap_or_else(|e| e.into_inner()).0;
        }
        match q.tags.pop_front() {
            Some(tag) => {
                q.bytes -= tag.payload.len();
                Pop::Tag(tag)
            }
            None if q.closed => Pop::Closed,
            None => Pop::Empty,
        }
    }

    /// Throw away everything older than the newest keyframe, headers apart.
    /// A destination that has just come back starts from now, not from the
    /// seconds it missed.
    pub fn skip_to_latest_keyframe(&self) {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let latest = q.tags.iter().rposition(starts_gop);
        let first = q.tags.iter().position(|t| !is_header(t));
        if first.is_some() && latest != first {
            drop_front(&mut q, latest);
        }
    }

    /// No more tags will come.
    pub fn close(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        self.ready.notify_all();
    }

    pub fn dropped(&self) -> Dropped {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).dropped
    }
}

/// Drop every media tag before index `until`, or every one when there is no
/// keyframe to stop at, which also starts skipping to the next.
fn drop_front(q: &mut Inner, until: Option<usize>) {
    let cut = until.unwrap_or(q.tags.len());
    let mut kept = VecDeque::with_capacity(q.tags.len());
    for (i, tag) in q.tags.drain(..).enumerate() {
        if i >= cut || is_header(&tag) {
            kept.push_back(tag);
        } else {
            q.bytes -= tag.payload.len();
            q.dropped.tags += 1;
        }
    }
    q.tags = kept;
    if until.is_none() {
        q.skipping = true;
    }
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
