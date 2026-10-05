//! Something drawn over the whole programme frame for a while: an effect
//! clip, the clip of a stinger, a luma matte wipe, a shader transition.
//!
//! A pass is not a source. It has no slot, no scene item and no tile; it is
//! put on the board when something asks for it, painted by the board's probe
//! after the transparent sources, and dropped when it says it has finished.
//! With no pass and no transparent source the probe is not there at all, so
//! a show that never fires an effect runs the graph it always ran.
//!
//! `paint` runs on the compositor's streaming thread, once a frame. It may
//! read and write memory it already has and do arithmetic. It may not wait:
//! a pass whose next picture is not ready draws the last one, or nothing.

use super::blend::Planes;
use std::sync::Arc;

pub trait Pass: Send + Sync {
    /// Paint onto the frame for running time `now`, in nanoseconds.
    fn paint(&self, frame: &mut Planes<'_>, now: u64);

    /// True once there is nothing more to draw. The board drops it on the
    /// next frame.
    fn finished(&self) -> bool {
        false
    }

    /// What a log line calls it.
    fn name(&self) -> &str;
}

/// The board's list: each pass with the number it was put on under.
#[derive(Default)]
pub struct Passes {
    next: u64,
    pub(super) list: Vec<(u64, Arc<dyn Pass>)>,
}

impl Passes {
    pub(super) fn add(&mut self, pass: Arc<dyn Pass>) -> u64 {
        self.next += 1;
        self.list.push((self.next, pass));
        self.next
    }

    pub(super) fn remove(&mut self, id: u64) {
        self.list.retain(|(i, _)| *i != id);
    }

    /// The passes to paint this frame, with the finished ones gone.
    pub(super) fn live(&mut self) -> Vec<Arc<dyn Pass>> {
        self.list.retain(|(_, p)| !p.finished());
        self.list.iter().map(|(_, p)| p.clone()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}
