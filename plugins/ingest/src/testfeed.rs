//! Real H.264 and AAC as the hub carries them, for tests: made by x264 and
//! the libav AAC encoder and turned into tags by `tagger`, exactly as an SRT
//! publisher's would be.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;

use crate::media_tag::MediaTag;
use crate::rtmp::Inlet;
use crate::tagger;

fn launch(live: bool, frames: Option<u32>, width: u32, height: u32) -> gst::Pipeline {
    gmx_netkit::init().unwrap();
    let count = frames.map(|n| format!("num-buffers={n}")).unwrap_or_default();
    let audio_count = frames.map(|n| format!("num-buffers={}", n * 48_000 / 30 / 1024 + 1)).unwrap_or_default();
    let line = format!(
        "videotestsrc is-live={live} {count} pattern=ball ! video/x-raw,width={width},height={height},framerate=30/1 \
         ! x264enc tune=zerolatency speed-preset=ultrafast key-int-max=30 bframes=0 bitrate=600 ! h264parse name=vp \
         audiotestsrc is-live={live} {audio_count} samplesperbuffer=1024 ! audio/x-raw,rate=48000,channels=2 ! avenc_aac ! aacparse name=ap"
    );
    gst::parse::launch(&line).unwrap().downcast::<gst::Pipeline>().unwrap()
}

fn attach(pipeline: &gst::Pipeline, inlet: Box<dyn Inlet>) {
    let to = tagger::share(inlet);
    let zero = Arc::new(tagger::Zero::default());
    for (parser, sink) in [("vp", tagger::video_sink(to.clone(), zero.clone())), ("ap", tagger::audio_sink(to, zero))] {
        pipeline.add(&sink).unwrap();
        pipeline.by_name(parser).unwrap().link(&sink).unwrap();
    }
}

struct Keep(Arc<Mutex<Vec<MediaTag>>>);

impl Inlet for Keep {
    fn tag(&mut self, tag: MediaTag) {
        self.0.lock().unwrap().push(tag);
    }
}

/// `frames` frames of 320x240 at 30 fps and the sound beside them, as tags
/// in the order the two sinks made them.
pub fn tags(frames: u32) -> Vec<MediaTag> {
    let got = Arc::new(Mutex::new(Vec::new()));
    let p = launch(false, Some(frames), 320, 240);
    attach(&p, Box::new(Keep(got.clone())));
    p.set_state(gst::State::Playing).unwrap();
    let _ = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(30), &[gst::MessageType::Eos, gst::MessageType::Error]);
    let _ = p.set_state(gst::State::Null);
    let mut tags = got.lock().unwrap().clone();
    tags.sort_by_key(|t| (t.timestamp_ms, !t.sequence_header));
    tags
}

/// A live 320x240 publisher into `inlet`, until the pipeline is set to Null.
pub fn live(inlet: Box<dyn Inlet>) -> gst::Pipeline {
    let p = launch(true, None, 320, 240);
    attach(&p, inlet);
    p.set_state(gst::State::Playing).unwrap();
    p
}

/// Decode an MPEG-TS file and count the pictures and the sound frames.
pub fn decode_ts(path: &std::path::Path) -> (u32, u32) {
    let line = format!(
        "filesrc location={} ! tsdemux name=d d. ! queue ! h264parse ! avdec_h264 ! fakesink name=v \
         d. ! queue ! aacparse ! avdec_aac ! fakesink name=a",
        path.display()
    );
    let p = gst::parse::launch(&line).unwrap().downcast::<gst::Pipeline>().unwrap();
    let counts = [Arc::new(Mutex::new(0u32)), Arc::new(Mutex::new(0u32))];
    for (name, n) in ["v", "a"].iter().zip(counts.iter()) {
        let n = n.clone();
        p.by_name(name).unwrap().static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            *n.lock().unwrap() += 1;
            gst::PadProbeReturn::Ok
        });
    }
    p.set_state(gst::State::Playing).unwrap();
    let _ = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(20), &[gst::MessageType::Eos, gst::MessageType::Error]);
    let _ = p.set_state(gst::State::Null);
    let v = *counts[0].lock().unwrap();
    let a = *counts[1].lock().unwrap();
    (v, a)
}
