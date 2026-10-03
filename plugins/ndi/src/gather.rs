//! Hold an NDI receiver's streams back until they have all arrived.
//!
//! `ndisrcdemux` adds a pad for a stream when its first frame comes, so a
//! sender's sound and picture turn up a few frames apart, in either order.
//! The receiver muxes them into Matroska for the core, and `matroskamux`
//! writes its header with the streams it has when data first reaches it: a
//! second pad after that is refused. So whichever stream came first was the
//! only one the mixer ever got, the picture with no sound or the sound with no
//! picture.
//!
//! Here each new pad is held until a video and an audio pad are both there,
//! or `WAIT` after the first one for a sender with only one of them, and then
//! all of them are linked at once. A held pad drops its buffers rather than
//! blocking, because one thread feeds both pads and a blocked one would stop
//! the other ever appearing. Events still pass, so caps and the segment are
//! waiting on the pad when it is linked.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;

/// How long a single stream waits for its partner.
pub const WAIT: Duration = Duration::from_secs(1);

type Attach = Arc<dyn Fn(&gst::Pipeline, &gst::Pad) + Send + Sync>;

#[derive(Default)]
struct Held {
    pads: Vec<gst::Pad>,
    released: Arc<AtomicBool>,
}

/// Link `demux`'s pads with `attach`, all together, once they have arrived.
pub fn connect(demux: &gst::Element, pipeline: &gst::Pipeline, attach: Attach) {
    let held = Arc::new(Mutex::new(Held::default()));
    let weak = pipeline.downgrade();
    demux.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        let mut h = held.lock().unwrap_or_else(|e| e.into_inner());
        if h.released.load(Ordering::SeqCst) {
            drop(h);
            attach(&pipeline, pad);
            return;
        }
        let released = Arc::clone(&h.released);
        pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST, move |_, _| {
            if released.load(Ordering::SeqCst) {
                gst::PadProbeReturn::Remove
            } else {
                gst::PadProbeReturn::Drop
            }
        });
        h.pads.push(pad.clone());
        if kinds(&h.pads) {
            release(&mut h, &pipeline, &attach);
        } else if h.pads.len() == 1 {
            let (held, weak, attach) = (Arc::clone(&held), pipeline.downgrade(), Arc::clone(&attach));
            std::thread::spawn(move || {
                std::thread::sleep(WAIT);
                let Some(pipeline) = weak.upgrade() else { return };
                let mut h = held.lock().unwrap_or_else(|e| e.into_inner());
                release(&mut h, &pipeline, &attach);
            });
        }
    });
}

/// Whether a picture and a sound are both among `pads`.
fn kinds(pads: &[gst::Pad]) -> bool {
    let has = |kind: &str| pads.iter().any(|p| p.name().starts_with(kind));
    has("video") && has("audio")
}

fn release(h: &mut Held, pipeline: &gst::Pipeline, attach: &Attach) {
    if h.released.swap(true, Ordering::SeqCst) {
        return;
    }
    for pad in h.pads.drain(..) {
        attach(pipeline, &pad);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(name: &str) -> gst::Pad {
        gst::Pad::builder(gst::PadDirection::Src).name(name).build()
    }

    #[test]
    fn a_picture_and_a_sound_are_both_needed_to_go_early() {
        gst::init().unwrap();
        assert!(!kinds(&[pad("video")]));
        assert!(!kinds(&[pad("audio")]));
        assert!(kinds(&[pad("audio"), pad("video")]));
    }
}
