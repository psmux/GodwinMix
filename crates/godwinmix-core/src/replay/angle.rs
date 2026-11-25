//! The chains that fill one angle's ring.
//!
//! A source angle encodes the source's own canvas sized picture and sound,
//! taken off its tees inside the source's pipeline:
//!
//! ```text
//!  {id}-vtee -> leaky queue -> videoconvert -> encoder -> parser -> ring
//!  {id}-atee -> leaky queue -> audioconvert -> audiorate -> encoder -> parser -> ring
//! ```
//!
//! The encoders are the ones the catalogue chose for the programme on this
//! machine, so a Mac uses VideoToolbox and a Pi the software encoder, with a
//! keyframe every second so the ring drops a second at a time and a seek
//! decodes at most a second of frames.
//!
//! The programme angle encodes nothing. It reads the programme encoder's own
//! output off `venc-tee` and `aenc-tee`, which every output already shares.

use super::branch::{Branch, Side};
use super::ring::Ring;
use crate::catalogue::apply::{self, Vars};
use crate::catalogue::select::Chosen;
use crate::gstutil::{self, make};
use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

/// What every source angle is encoded with.
#[derive(Clone)]
pub struct EncodeWith {
    pub video: Chosen,
    pub audio: Chosen,
    pub vars: Vars,
}

impl EncodeWith {
    /// The programme's choice, with a keyframe every second and the bitrate
    /// replay was asked for.
    pub fn for_replay(mut self, bitrate_kbps: u32) -> Self {
        self.vars.keyframe_secs = 1;
        self.vars.keyframe_frames = self.vars.fps.max(1);
        if bitrate_kbps > 0 {
            self.vars.video_bitrate_kbps = i64::from(bitrate_kbps);
        }
        self
    }
}

/// The two chains of one angle. Dropping it takes both off their tees.
pub struct AngleBranches {
    pub video: Branch,
    pub audio: Option<Branch>,
    pub encoder: String,
}

impl AngleBranches {
    pub fn dropped(&self) -> u64 {
        self.video.dropped()
    }
}

/// A queue a few frames deep that drops the oldest when full.
fn leaky(name: &str, buffers: u32) -> Result<gst::Element> {
    let q = gstutil::queue_preview(name)?;
    q.set_property("max-size-buffers", buffers);
    Ok(q)
}

fn parser(name: Option<&str>, tag: &str) -> Result<Vec<gst::Element>> {
    let Some(name) = name.filter(|n| crate::probe::exists(n)) else { return Ok(Vec::new()) };
    let el = make(name, tag)?;
    // Parameter sets in front of every keyframe, so any GOP decodes alone.
    crate::probe::set_int(&el, "config-interval", -1);
    Ok(vec![el])
}

/// Encode one source's tees into `ring`.
pub fn source(
    pipeline: &gst::Pipeline,
    vtee: &gst::Element,
    atee: Option<&gst::Element>,
    ring: &Arc<Mutex<Ring>>,
    with: &EncodeWith,
    tag: &str,
) -> Result<AngleBranches> {
    let venc = make(&with.video.element, &format!("{tag}-venc"))?;
    apply::apply(&venc, &with.video.properties, &with.vars);
    apply::apply_keyframe(&venc, with.video.keyframe.as_ref(), &with.vars);
    let mut body = vec![make("videoconvert", &format!("{tag}-vconv"))?, venc];
    body.extend(parser(with.video.parser.as_deref(), &format!("{tag}-vparse"))?);
    let video = Branch::attach(pipeline, vtee, leaky(&format!("{tag}-vq"), 4)?, body, ring, Side::Video, &format!("{tag}-v"))?;
    let audio = match atee {
        Some(atee) => {
            let aenc = make(&with.audio.element, &format!("{tag}-aenc"))?;
            apply::apply(&aenc, &with.audio.properties, &with.vars);
            let rate = make("audiorate", &format!("{tag}-arate"))?;
            crate::probe::set_bool(&rate, "skip-to-first", true);
            let mut body = vec![make("audioconvert", &format!("{tag}-aconv"))?, rate, aenc];
            body.extend(parser(with.audio.parser.as_deref(), &format!("{tag}-aparse"))?);
            Some(Branch::attach(pipeline, atee, leaky(&format!("{tag}-aq"), 50)?, body, ring, Side::Audio, &format!("{tag}-a"))?)
        }
        None => None,
    };
    Ok(AngleBranches { video, audio, encoder: with.video.element.clone() })
}

/// Read the programme encoder's output into `ring`. Nothing is encoded.
pub fn programme(
    pipeline: &gst::Pipeline,
    venc_tee: &gst::Element,
    aenc_tee: &gst::Element,
    ring: &Arc<Mutex<Ring>>,
    tag: &str,
) -> Result<AngleBranches> {
    let video = Branch::attach(pipeline, venc_tee, leaky(&format!("{tag}-vq"), 30)?, Vec::new(), ring, Side::Video, &format!("{tag}-v"))?;
    let audio = Branch::attach(pipeline, aenc_tee, leaky(&format!("{tag}-aq"), 100)?, Vec::new(), ring, Side::Audio, &format!("{tag}-a"))?;
    Ok(AngleBranches { video, audio: Some(audio), encoder: "shared".to_string() })
}
