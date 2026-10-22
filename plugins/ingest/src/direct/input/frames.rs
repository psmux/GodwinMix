//! Appsinks for the audio a classic FLV tag cannot carry: AC-3, E-AC-3 and
//! MPEG layer II, each frame framed as an enhanced RTMP audio body
//! (`crate::exaudio`). MP3 keeps the classic FLV id it has always had.
//!
//! These frames carry their own headers, so there is no sequence header to
//! send first. The tagger's sinks do H.264, HEVC and AAC.

use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use super::streams::Gate;
use crate::exaudio;
use crate::media_tag::{MediaTag, TagKind};
use crate::rtmp::Inlet;
use crate::tagger::{self, Shared, Zero};

/// Classic FLV's MP3: sound format 2, 44.1 kHz, 16 bit, stereo. The flags
/// are what every RTMP encoder writes; the frame header has the truth.
const FLV_MP3: [u8; 1] = [0x2F];

pub fn ac3_sink(to: Shared, zero: Arc<Zero>) -> gst::Element {
    let caps = "audio/x-ac3,alignment=frame; audio/x-eac3,alignment=frame";
    sink(caps, to, zero, |caps| {
        let eac3 = caps.structure(0).is_some_and(|s| s.name() == "audio/x-eac3");
        exaudio::prefix(if eac3 { exaudio::EAC3 } else { exaudio::AC3 }).to_vec()
    })
}

pub fn mpeg_sink(to: Shared, zero: Arc<Zero>) -> gst::Element {
    sink("audio/mpeg,mpegversion=1,parsed=true", to, zero, |caps| {
        let layer = caps.structure(0).and_then(|s| s.get::<i32>("layer").ok()).unwrap_or(3);
        if layer == 3 { FLV_MP3.to_vec() } else { exaudio::prefix(exaudio::MPEG).to_vec() }
    })
}

/// MPEG-2 video, one picture a buffer as `mpegvideoparse` frames it (which
/// `parsebin` has already put in), as `mp2v` tags (`crate::mp2v`).
pub fn mpeg2_sink(to: Shared, zero: Arc<Zero>) -> gst::Element {
    let caps = "video/mpeg,mpegversion=[1,2],systemstream=false".parse::<gst::Caps>().expect("the caps parse");
    gst_app::AppSink::builder()
        .caps(&caps)
        .sync(false)
        .max_buffers(64)
        .drop(true)
        .callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let Some(tag) = picture(&sample, &zero) else { return Ok(gst::FlowSuccess::Ok) };
                    let mut inlet = to.lock().unwrap_or_else(|e| e.into_inner());
                    let Some(inlet) = inlet.as_mut() else { return Err(gst::FlowError::Eos) };
                    inlet.tag(tag);
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        )
        .build()
        .upcast()
}

fn picture(sample: &gst::Sample, zero: &Zero) -> Option<MediaTag> {
    let buffer = sample.buffer()?;
    let dts = buffer.dts().or(buffer.pts())?;
    let segment = sample.segment()?.downcast_ref::<gst::format::Time>()?;
    let ms = zero.ms(segment.to_running_time(dts)?);
    let cts = buffer.pts().map_or(0, |pts| pts.mseconds() as i64 - dts.mseconds() as i64);
    let map = buffer.map_readable().ok()?;
    Some(crate::mp2v::tag(map.as_slice(), ms, cts as i32))
}

/// Where one sound track's tags go: on to `to`, each with the track its
/// gate holds now, which moves when a track before it goes.
pub fn numbered(to: Shared, gate: Arc<Gate>) -> Shared {
    tagger::share(Box::new(Numbered { to, gate }))
}

struct Numbered {
    to: Shared,
    gate: Arc<Gate>,
}

impl Inlet for Numbered {
    fn tag(&mut self, mut tag: MediaTag) {
        tag.track = self.gate.track.load(std::sync::atomic::Ordering::Relaxed);
        if let Some(inlet) = self.to.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            inlet.tag(tag);
        }
    }
}

fn sink(caps: &str, to: Shared, zero: Arc<Zero>, head: fn(&gst::CapsRef) -> Vec<u8>) -> gst::Element {
    let caps = caps.parse::<gst::Caps>().expect("the caps above parse");
    gst_app::AppSink::builder()
        .caps(&caps)
        .sync(false)
        .max_buffers(64)
        .drop(true)
        .callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let Some(tag) = frame(&sample, &zero, head) else { return Ok(gst::FlowSuccess::Ok) };
                    let mut inlet = to.lock().unwrap_or_else(|e| e.into_inner());
                    let Some(inlet) = inlet.as_mut() else { return Err(gst::FlowError::Eos) };
                    inlet.tag(tag);
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        )
        .build()
        .upcast()
}

fn frame(sample: &gst::Sample, zero: &Zero, head: fn(&gst::CapsRef) -> Vec<u8>) -> Option<MediaTag> {
    let buffer = sample.buffer()?;
    let at = buffer.dts().or(buffer.pts())?;
    let segment = sample.segment()?.downcast_ref::<gst::format::Time>()?;
    let ms = zero.ms(segment.to_running_time(at)?);
    let map = buffer.map_readable().ok()?;
    let mut payload = head(sample.caps()?);
    payload.extend_from_slice(map.as_slice());
    Some(MediaTag { track: 0, kind: TagKind::Audio, timestamp_ms: ms, keyframe: false, sequence_header: false, payload: Arc::from(payload) })
}
