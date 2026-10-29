//! Deinterlacing on the card, for a CUDA scale (`deinterlace.cu`).
//!
//! The stage is the picture converted to I420 on the card, which makes a
//! buffer of its own, and a probe on it that runs the kernel on each plane
//! and says the picture is progressive. Nothing leaves the card. Before
//! this, the picture went down to system memory for `deinterlace`, which on
//! a headend of fifty channels was nine cores of YADIF and five more of
//! copies.
//!
//! One picture out for each picture in, the top field kept, as the CPU
//! stage was set (`fields=top`). `linear` rebuilds every line of the other
//! field; `yadif` and `greedyh` keep still detail woven and rebuild only
//! where the fields comb.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;

use super::cuda_api;
use super::cuda_launch::run;
use super::cuda_scratch::Scratch;

/// How far outside its neighbours a sample may sit and still be detail.
const THRESHOLD: i32 = 10;

/// The flags that say a buffer holds fields.
const FIELDS: gst_video::VideoBufferFlags = gst_video::VideoBufferFlags::INTERLACED
    .union(gst_video::VideoBufferFlags::TFF)
    .union(gst_video::VideoBufferFlags::RFF)
    .union(gst_video::VideoBufferFlags::ONEFIELD);

/// Whether this machine can deinterlace on the card: the CUDA calls are
/// there. The kernel itself loads with the first picture.
pub fn available() -> bool {
    cuda_api::api().is_ok()
}

/// What the stage knows from its caps, and the copy it reads from.
#[derive(Default)]
struct State {
    /// The layout of the I420 picture the kernel works on.
    info: Option<gst_video::VideoInfo>,
    /// The interlace mode of the pictures coming in.
    mode: String,
    /// Whether the picture now in the converter came in flagged as fields.
    fields: bool,
    scratch: Scratch,
    failed: bool,
}

/// Run the kernel on every picture that came in through `entry` holding
/// fields, all of them when `always`, on its way out through `exit`, and
/// say downstream that they are progressive.
///
/// The interlace mode and the fields flag are read going in: the converter
/// between the two pads keeps neither. It hands each picture on in the call
/// that brought it, so the flag noted at `entry` is that picture's at `exit`.
pub fn install(entry: &gst::Pad, exit: &gst::Pad, always: bool, linear: bool) {
    let state = Arc::new(Mutex::new(State::default()));
    let lock = |s: &Arc<Mutex<State>>| s.clone();
    let st = lock(&state);
    entry.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM | gst::PadProbeType::BUFFER, move |_, info| {
        let mut st = st.lock().unwrap_or_else(|p| p.into_inner());
        match &info.data {
            Some(gst::PadProbeData::Buffer(b)) => st.fields = raw_flags(b) & INTERLACED != 0,
            Some(gst::PadProbeData::Event(e)) => {
                if let gst::EventView::Caps(c) = e.view() {
                    st.mode = c.caps().structure(0).and_then(|s| s.get::<&str>("interlace-mode").ok()).unwrap_or("progressive").to_string();
                }
            }
            _ => {}
        }
        gst::PadProbeReturn::Ok
    });
    let st = lock(&state);
    exit.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        let Some(gst::PadProbeData::Event(event)) = &info.data else { return gst::PadProbeReturn::Ok };
        let gst::EventView::Caps(c) = event.view() else { return gst::PadProbeReturn::Ok };
        let progressive = {
            let mut st = st.lock().unwrap_or_else(|p| p.into_inner());
            st.info = gst_video::VideoInfo::from_caps(c.caps()).ok();
            progressive(c.caps())
        };
        info.data = Some(gst::PadProbeData::Event(gst::event::Caps::new(&progressive)));
        gst::PadProbeReturn::Ok
    });
    exit.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        let mut st = state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(buffer) = info.buffer_mut() {
            deinterlace(buffer.make_mut(), &mut st, always, linear);
        }
        gst::PadProbeReturn::Ok
    });
}

const INTERLACED: u32 = gst_video::VideoBufferFlags::INTERLACED.bits();

/// A buffer's flags as they are: `flags()` drops the bits GStreamer core
/// has no name for, and the video library's are exactly those.
pub(super) fn raw_flags(b: &gst::BufferRef) -> u32 {
    // SAFETY: reading a field of a buffer we hold a reference to.
    unsafe { (*b.as_ptr()).mini_object.flags }
}

/// The caps said progressive, as every picture out of the stage is.
fn progressive(caps: &gst::CapsRef) -> gst::Caps {
    let mut changed = caps.to_owned();
    if let Some(s) = changed.make_mut().structure_mut(0) {
        s.set("interlace-mode", "progressive");
        s.remove_field("field-order");
    }
    changed
}

fn deinterlace(buffer: &mut gst::BufferRef, st: &mut State, always: bool, linear: bool) {
    let fields = always || st.mode == "interleaved" || (st.mode == "mixed" && st.fields);
    let Some(info) = st.info.clone() else { return };
    if fields {
        if let Err(e) = run(buffer, &info, &mut st.scratch, if linear { -1 } else { THRESHOLD }) {
            if !std::mem::replace(&mut st.failed, true) {
                eprintln!("transcode: deinterlacing on the card failed, these pictures stay woven: {e}");
            }
        }
    }
    // SAFETY: the buffer is writable (`make_mut` above).
    unsafe { (*buffer.as_mut_ptr()).mini_object.flags &= !FIELDS.bits() }
}
