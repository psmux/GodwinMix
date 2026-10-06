//! No buffer reaches the player's muxer until every stream it will carry
//! has told the muxer its caps.
//!
//! `mpegtsmux` writes its first PMT with the streams whose caps it has when
//! it first muxes. `aacparse` passes its first frame on at once and
//! `h264parse` only once it has read the first picture, so on a busy machine
//! the sound got there first and the first PMT named the sound alone: a
//! debug log of the failing run showed "PMT for program 1 has 1 streams"
//! with the picture's pad "caps were not set yet", and the player's
//! `tsdemux` offered no picture at all. Holding `VideoFirst` order going in
//! did not help, because the two parsers run on their own queues' threads.
//!
//! So each sink pad's buffers wait, while its caps go through, until every
//! pad has caps. Only the queue threads in front of the muxer wait, never the
//! demuxer, and `open` lets everything through if a stream never says what
//! it is, so the player gets something rather than nothing.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;

pub struct CapsGate {
    held: Mutex<Vec<(gst::Pad, gst::PadProbeId)>>,
    want: usize,
}

impl CapsGate {
    /// Hold every sink pad `mux` has or gets until `want` of them have caps.
    pub fn hold(mux: &gst::Element, want: usize) -> Arc<CapsGate> {
        let gate = Arc::new(CapsGate { held: Mutex::new(Vec::new()), want });
        for pad in mux.sink_pads() {
            gate.watch(&pad);
        }
        let weak = Arc::downgrade(&gate);
        mux.connect_pad_added(move |_, pad| {
            if let Some(gate) = weak.upgrade().filter(|_| pad.direction() == gst::PadDirection::Sink) {
                gate.watch(pad);
            }
        });
        gate
    }

    fn watch(self: &Arc<Self>, pad: &gst::Pad) {
        let weak = Arc::downgrade(self);
        let mux = pad.parent_element();
        let id = pad.add_probe(gst::PadProbeType::BLOCK | gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST, move |_, _| {
            let ready = mux.as_ref().is_some_and(|m| m.sink_pads().iter().filter(|p| p.current_caps().is_some()).count() >= weak.upgrade().map_or(0, |g| g.want));
            // Opening takes every hold off, this one too.
            if let Some(gate) = weak.upgrade().filter(|_| ready) {
                gate.open();
            }
            gst::PadProbeReturn::Ok
        });
        if let Some(id) = id {
            self.held.lock().unwrap_or_else(|e| e.into_inner()).push((pad.clone(), id));
        }
    }

    /// Let everything through from now on.
    pub fn open(&self) {
        let held = std::mem::take(&mut *self.held.lock().unwrap_or_else(|e| e.into_inner()));
        for (pad, id) in held {
            pad.remove_probe(id);
        }
    }

    pub fn is_open(&self) -> bool {
        self.held.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }
}
