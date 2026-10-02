//! What the key hands on: the keyed frames given to the board, kept so their
//! block arrays can be used again, and the gap the compositor gets instead.

use crate::overlay::keyed::Keyed;
use gstreamer as gst;
use std::sync::Arc;

/// An empty buffer flagged as a gap, in place of the frame the board draws.
///
/// Dropping the frame instead would leave the compositor pad with nothing
/// for its time: a pad that has had buffers and then stops is waited for, a
/// whole upstream latency of programme at a time. A gap says the time has
/// passed with nothing to draw, so the compositor neither waits for this pad
/// nor goes on drawing the last frame it was given.
pub(super) fn gap(frame: &gst::BufferRef) -> gst::Buffer {
    let mut out = gst::Buffer::new();
    {
        let b = out.get_mut().expect("a new buffer is writable");
        b.set_pts(frame.pts());
        b.set_dts(frame.dts());
        b.set_duration(frame.duration());
        b.set_flags(gst::BufferFlags::GAP | gst::BufferFlags::DROPPABLE);
    }
    out
}

/// The last few keyed frames handed out.
#[derive(Default)]
pub(super) struct Handed(Vec<Arc<Keyed>>);

impl Handed {
    pub(super) fn push(&mut self, k: Arc<Keyed>) {
        self.0.push(k);
    }

    /// Keep only the newest `n`.
    pub(super) fn truncate_front(&mut self, n: usize) {
        let extra = self.0.len().saturating_sub(n);
        self.0.drain(..extra);
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, Arc<Keyed>> {
        self.0.iter()
    }

    pub(super) fn remove(&mut self, i: usize) -> Arc<Keyed> {
        self.0.remove(i)
    }
}
