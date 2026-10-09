//! The half of stopping an encode chain that waits.
//!
//! Unlinking the chain from the raw tee waits for nothing, and after it no
//! frame reaches the chain. Taking the chain to NULL is different: each
//! element joins its streaming thread, and the encoder's thread is wherever
//! its driver has it. Quick Sync on this project's Windows laptop corrupts
//! the heap of a process that loads it about one load in four, and nothing
//! promises that its teardown returns in good time either. So this part is
//! handed back to the caller as a value, and `Encoder::stop` runs it on a
//! thread from `mixer::offload`, which logs when it overruns.

use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::debug;

/// A chain already unlinked from its tee, still to be taken down.
pub struct Retire {
    pub tag: &'static str,
    pub tee: gst::Element,
    /// The tee pad it hung off, when it was attached.
    pub pad: Option<gst::Pad>,
    /// Head first, as the chain keeps them.
    pub chain: Vec<gst::Element>,
}

impl Retire {
    /// Tail first to NULL, then the tee pad back to the tee. The pad goes
    /// last because releasing it takes its stream lock, and a push already
    /// inside the head queue holds that until the queue lets it go, which the
    /// head going to NULL makes it do.
    pub fn run(self) {
        for el in self.chain.iter().rev() {
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
        }
        if let Some(pad) = &self.pad {
            self.tee.release_request_pad(pad);
        }
        debug!(chain = self.tag, "encode chain detached");
    }
}
