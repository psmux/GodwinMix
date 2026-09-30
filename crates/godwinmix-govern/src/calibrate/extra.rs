//! The runs after the encoders: a scale, a decode, an audio encode.

use super::pipeline::{make, sink, video_source};
use super::probe::{encode_chain, timed, Baselines, ENCODER};
use super::{AudioCandidate, Candidate};
use crate::calibration::{mpix, AudioCal, Point};
use godwinmix_protocol::rendition::Fps;
use gstreamer as gst;
use gstreamer::prelude::*;

pub fn scale(base: &Baselines, frames: u32, notes: &mut Vec<String>) -> Option<f64> {
    let (from, to) = ((1920, 1080, 30), (1280, 720, 30));
    let els = (|| {
        let mut v = video_source(from.0, from.1, from.2, frames)?;
        v.push(make("videoscale")?);
        v.push(make("videoconvert")?);
        let f = make("capsfilter")?;
        f.set_property("caps", gst::Caps::builder("video/x-raw").field("format", "NV12").field("width", to.0 as i32).field("height", to.1 as i32).build());
        v.push(f);
        v.push(sink()?);
        Ok(v)
    })();
    match timed(els, 2, base.get(from)) {
        Ok((cpu, _)) => Some(f64::from(cpu) / (mpix(from.0, from.1, Fps::whole(from.2)) + mpix(to.0, to.1, Fps::whole(to.2)))),
        Err(e) => {
            notes.push(format!("scaling could not be timed: {e}"));
            None
        }
    }
}

/// Encode, parse and decode at `point`'s shape: the whole, in thousandths.
pub fn round_trip(c: &Candidate, point: &Point, base: &Baselines, frames: u32, notes: &mut Vec<String>) -> Option<u32> {
    let s = (point.width, point.height, point.fps.num);
    let tail = [c.parser.as_deref()?, c.decoder.as_deref()?];
    match timed(encode_chain(c, s, frames, None, &tail), ENCODER, base.get(s)) {
        Ok((cpu, _)) => Some(cpu),
        Err(e) => {
            notes.push(format!("decoding {} could not be timed: {e}", c.slot.id));
            None
        }
    }
}

pub fn audio(a: &AudioCandidate, notes: &mut Vec<String>) -> Option<AudioCal> {
    let els = (|| {
        let src = make("audiotestsrc")?;
        src.set_property("num-buffers", 50i32);
        src.set_property("samplesperbuffer", 960i32);
        let f = make("capsfilter")?;
        f.set_property("caps", gst::Caps::builder("audio/x-raw").field("format", "S16LE").field("rate", 48000i32).field("channels", 2i32).field("layout", "interleaved").build());
        let enc = make(&a.element)?;
        (a.configure)(&enc);
        Ok(vec![src, f, make("audioconvert")?, make("audioresample")?, enc, sink()?])
    })();
    match timed(els, 4, (0.0, 0.0)) {
        Ok((cpu, _)) => Some(AudioCal { id: a.id.clone(), codec: a.codec, element: a.element.clone(), cpu_millicores: cpu }),
        Err(e) => {
            notes.push(format!("{} ({}) could not be timed: {e}", a.id, a.element));
            None
        }
    }
}
