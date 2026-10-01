//! Which elementary stream is carried, and which wait.
//!
//! A tag stream carries one video and one audio. Every stream the demuxer
//! offers gets a chain of its own, but only the one holding its kind's slot
//! passes buffers; the others stand by behind a probe that drops them, and
//! cost a parser that sees no data.
//!
//! When a sender restarts with a new layout (new PIDs, a new program, a
//! stream added to the PMT), `tsdemux` adds the new streams first and then
//! removes the old ones. So a stream that arrives while its slot is held
//! stands by, and when the holder's pad goes away the first stream waiting
//! for that slot takes it. A second video stream in the same live program
//! keeps standing by, and is named in the input's error as before.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Video,
    Audio,
    /// Carried nowhere: teletext, MPEG-2 video, anything without a sink.
    None,
}

struct Stream {
    pad: gst::Pad,
    slot: Slot,
    /// The elements after the pad, removed with it.
    chain: Vec<gst::Element>,
    /// Buffers pass while this is set.
    on: Arc<AtomicBool>,
    /// Why it is left out, while it is.
    note: Option<String>,
}

#[derive(Default)]
pub struct Streams(Mutex<Vec<Stream>>);

impl Streams {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Stream>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Take `pad` on, with `chain` after it: holding its slot if the slot is
    /// free, standing by if not. Answers the gate a probe on the pad reads.
    pub fn add(&self, pad: &gst::Pad, slot: Slot, what: &str, chain: Vec<gst::Element>) -> Arc<AtomicBool> {
        let mut all = self.lock();
        let held = slot != Slot::None && all.iter().any(|s| s.slot == slot && s.on.load(Ordering::Relaxed));
        let on = Arc::new(AtomicBool::new(slot != Slot::None && !held));
        let note = match slot {
            Slot::None => Some(format!("{what}, which a direct show does not carry")),
            _ if held => Some(format!("a second {} stream ({what}); the first is taken", if slot == Slot::Video { "video" } else { "audio" })),
            _ => None,
        };
        all.push(Stream { pad: pad.clone(), slot, chain, on: on.clone(), note });
        on
    }

    /// `pad` went away. If it held a slot, the first stream standing by for
    /// that slot takes it. Answers the elements to take out of the pipeline.
    pub fn removed(&self, pad: &gst::Pad) -> Vec<gst::Element> {
        let mut all = self.lock();
        let Some(at) = all.iter().position(|s| &s.pad == pad) else { return Vec::new() };
        let gone = all.remove(at);
        if gone.slot != Slot::None && gone.on.load(Ordering::Relaxed) {
            if let Some(next) = all.iter_mut().find(|s| s.slot == gone.slot) {
                next.on.store(true, Ordering::Relaxed);
                next.note = None;
            }
        }
        gone.chain
    }

    /// What is left out right now, each said once.
    pub fn notes(&self) -> Vec<String> {
        self.lock().iter().filter_map(|s| s.note.clone()).collect()
    }
}

/// Drop `pad`'s buffers while `on` is clear. Events pass, so a stream that
/// takes over already has its caps and segment.
pub fn gate(pad: &gst::Pad, on: Arc<AtomicBool>) {
    pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST, move |_, _| {
        if on.load(Ordering::Relaxed) { gst::PadProbeReturn::Ok } else { gst::PadProbeReturn::Drop }
    });
}

/// Take `chain` out of `pipeline` off the streaming thread, which is the one
/// removing the pad and must not wait on a state change.
pub fn take_out(pipeline: &gst::Pipeline, chain: Vec<gst::Element>) {
    if chain.is_empty() {
        return;
    }
    let weak = pipeline.downgrade();
    pipeline.call_async(move |_| {
        let Some(pipeline) = weak.upgrade() else { return };
        for e in &chain {
            let _ = e.set_state(gst::State::Null);
            let _ = pipeline.remove(e);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(name: &str) -> gst::Pad {
        gst::Pad::builder(gst::PadDirection::Src).name(name).build()
    }

    #[test]
    fn a_stream_waiting_for_its_slot_takes_it_when_the_holder_goes() {
        gst::init().unwrap();
        let s = Streams::default();
        let (v1, a1, v2, a2, a3) = (pad("v1"), pad("a1"), pad("v2"), pad("a2"), pad("a3"));
        let on_v1 = s.add(&v1, Slot::Video, "video/x-h264", Vec::new());
        let on_a1 = s.add(&a1, Slot::Audio, "audio/mpeg", Vec::new());
        let on_v2 = s.add(&v2, Slot::Video, "video/x-h264", Vec::new());
        let on_a2 = s.add(&a2, Slot::Audio, "audio/mpeg", Vec::new());
        let on_a3 = s.add(&a3, Slot::Audio, "audio/mpeg", Vec::new());
        assert!(on_v1.load(Ordering::Relaxed) && on_a1.load(Ordering::Relaxed));
        assert!(!on_v2.load(Ordering::Relaxed) && !on_a2.load(Ordering::Relaxed));
        assert_eq!(s.notes().len(), 3, "{:?}", s.notes());
        s.removed(&a1);
        s.removed(&v1);
        assert!(on_v2.load(Ordering::Relaxed) && on_a2.load(Ordering::Relaxed) && !on_a3.load(Ordering::Relaxed));
        assert_eq!(s.notes(), ["a second audio stream (audio/mpeg); the first is taken"]);
    }

    #[test]
    fn a_stream_left_out_is_forgotten_when_it_goes() {
        gst::init().unwrap();
        let s = Streams::default();
        let (v1, t) = (pad("v1"), pad("t"));
        s.add(&v1, Slot::Video, "video/x-h264", Vec::new());
        assert!(!s.add(&t, Slot::None, "private/teletext", Vec::new()).load(Ordering::Relaxed));
        assert_eq!(s.notes(), ["private/teletext, which a direct show does not carry"]);
        s.removed(&t);
        assert!(s.notes().is_empty());
    }
}
