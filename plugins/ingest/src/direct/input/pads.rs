//! Demuxed streams to tags: `parsebin` after the transport, then for each
//! stream it finds, a parser and an appsink that frames each access unit the
//! way the hub carries it. Nothing is decoded.
//!
//! ```text
//!   transport ──► parsebin ─┬─► h264parse | h265parse ──► appsink (video tags)
//!                           ├─► aacparse | ac3parse | mpegaudioparse ──► appsink (audio tags)
//!                           ├─► appsink (MPEG-2 video, already framed by parsebin, as `mp2v` tags)
//!                           └─► fakesink (teletext, anything else)
//! ```
//!
//! MPEG-2 video is carried only when the show converts it, and every sound
//! track only when it converts them (`Carry`); otherwise one video and one
//! audio stream are carried, the first of each to appear.
//! A second one of a kind stands by until the first one's pad goes away,
//! which is what a sender restarting with a new layout looks like
//! (`streams.rs`). What is left out is named in the input's error.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;

use super::frames;
use super::numbering::meta;
use super::spec::Carry;
use super::streams::{gate, take_out, Gate, Slot, Streams};
use crate::tagger::{self, Zero};

/// What every pad of one session shares.
pub struct Pads {
    pub to: tagger::Shared,
    pub zero: Arc<Zero>,
    /// Play at the clock's pace (a file), rather than as fast as it comes.
    pub sync: bool,
    /// The MPEG-TS program to take, set on any `tsdemux` parsebin makes.
    pub program: Option<u16>,
    pub carry: Carry,
    streams: Arc<Streams>,
    /// What went wrong putting the demuxer in, said once.
    pub note: Mutex<Vec<String>>,
}

impl Pads {
    pub fn new(to: tagger::Shared, sync: bool, program: Option<u16>, carry: Carry) -> Arc<Pads> {
        Arc::new(Pads {
            to,
            zero: Arc::new(Zero::default()),
            sync,
            program,
            carry,
            streams: Arc::new(Streams::new(carry.tracks)),
            note: Mutex::new(Vec::new()),
        })
    }

    /// Everything left out right now, joined for a message.
    pub fn notes(&self) -> String {
        let mut all = self.note.lock().unwrap_or_else(|e| e.into_inner()).clone();
        all.extend(self.streams.notes());
        all.join("; ")
    }

    /// What is carried now, for the input's stats.
    pub fn tracks(&self) -> Vec<super::stats::TrackInfo> {
        self.streams.tracks()
    }
}

/// Put a `parsebin` after `pad` and attach whatever it finds.
pub fn parse_into(pipeline: &gst::Pipeline, pad: &gst::Pad, pads: &Arc<Pads>) -> Result<(), String> {
    let parse = make("parsebin")?;
    if let Some(program) = pads.program {
        parse.connect("deep-element-added", false, move |args| {
            let element = args.get(2).and_then(|v| v.get::<gst::Element>().ok());
            if let Some(e) = element.filter(|e| e.factory().is_some_and(|f| f.name() == "tsdemux")) {
                e.set_property("program-number", i32::from(program));
            }
            None
        });
    }
    let (weak, shared) = (pipeline.downgrade(), pads.clone());
    parse.connect_pad_removed(move |_, pad| {
        if let Some(pipeline) = weak.upgrade() {
            take_out(&pipeline, shared.streams.removed(pad));
        }
    });
    let (weak, shared) = (pipeline.downgrade(), pads.clone());
    parse.connect_pad_added(move |_, pad| {
        if let Some(pipeline) = weak.upgrade() {
            if let Err(e) = attach(&pipeline, pad, &shared) {
                shared.note.lock().unwrap_or_else(|e| e.into_inner()).push(e);
            }
        }
    });
    pipeline.add(&parse).map_err(|e| e.to_string())?;
    parse.sync_state_with_parent().map_err(|e| e.to_string())?;
    let into = parse.static_pad("sink").ok_or("parsebin has no sink pad")?;
    pad.link(&into).map_err(|e| format!("the transport would not link to the demuxer: {e:?}"))?;
    Ok(())
}

/// One elementary stream, to the sink for its kind, or to a fakesink.
fn attach(pipeline: &gst::Pipeline, pad: &gst::Pad, pads: &Pads) -> Result<(), String> {
    // parsebin says what a pad carries in its stream object; the pad's own
    // caps may not be set yet when it is announced.
    let caps = pad.current_caps().or_else(|| pad.stream().and_then(|s| s.caps())).unwrap_or_else(|| pad.query_caps(None));
    let s = caps.structure(0).map(|s| s.to_owned());
    let name = s.as_ref().map(|s| s.name().to_string()).unwrap_or_default();
    // MPEG-1 audio is layers I to III; anything else called audio/mpeg is AAC.
    let layered = s.as_ref().is_some_and(|s| s.get::<i32>("mpegversion").ok() == Some(1) || s.has_field("layer"));
    let slot = if name.starts_with("video/") { Slot::Video } else { Slot::Audio };
    let mpeg2 = name == "video/mpeg" && pads.carry.mpeg2 && s.as_ref().is_some_and(|s| s.get::<bool>("systemstream").ok() != Some(true));
    let numbered = Arc::new(Gate::default());
    let to = match slot == Slot::Audio && pads.carry.tracks {
        true => frames::numbered(pads.to.clone(), numbered.clone()),
        false => pads.to.clone(),
    };
    let zero = pads.zero.clone();
    let (parser, sink) = match name.as_str() {
        "video/x-h264" => (Some("h264parse"), tagger::video_sink(to, zero)),
        "video/x-h265" => (Some("h265parse"), tagger::hevc_sink(to, zero)),
        "video/mpeg" if mpeg2 => (None, frames::mpeg2_sink(to, zero)),
        "audio/mpeg" if !layered => (Some("aacparse"), tagger::audio_sink(to, zero)),
        "audio/x-ac3" | "audio/x-eac3" => (Some("ac3parse"), frames::ac3_sink(to, zero)),
        "audio/mpeg" => (Some("mpegaudioparse"), frames::mpeg_sink(to, zero)),
        _ => return discard(pipeline, pad, pads, &name),
    };
    sink.set_property("sync", pads.sync);
    // Never wait to preroll: srtsrc and urisourcebin are not live, and a
    // video sink waiting for its first keyframe would hold the demuxer, back
    // up the audio, and stall the transport until the sender gives up.
    sink.set_property("async", false);
    // No queue of our own: parsebin already puts a multiqueue after a
    // demuxer, and two more threads per input is 400 for 200 shows.
    // parsebin has framed MPEG-2 already, and nothing needs it converted.
    let chain: Vec<gst::Element> = parser.map(make).transpose()?.into_iter().chain([sink]).collect();
    let on = pads.streams.add_with(pad, slot, &name, chain.clone(), meta(pad), numbered);
    gate(pad, on, Arc::downgrade(&pads.streams));
    link(pipeline, pad, &chain.iter().collect::<Vec<_>>())
}

/// Send a stream nobody carries to a fakesink; `Streams` says why.
fn discard(pipeline: &gst::Pipeline, pad: &gst::Pad, pads: &Pads, what: &str) -> Result<(), String> {
    let sink = make("fakesink")?;
    sink.set_property("sync", false);
    sink.set_property("async", false);
    pads.streams.add(pad, Slot::None, what, vec![sink.clone()]);
    link(pipeline, pad, &[&sink])
}

fn link(pipeline: &gst::Pipeline, pad: &gst::Pad, chain: &[&gst::Element]) -> Result<(), String> {
    pipeline.add_many(chain.iter().copied()).map_err(|e| e.to_string())?;
    gst::Element::link_many(chain.iter().copied()).map_err(|e| e.to_string())?;
    for e in chain {
        let _ = e.sync_state_with_parent();
    }
    let into = chain[0].static_pad("sink").ok_or("no sink pad")?;
    pad.link(&into).map_err(|e| format!("would not link: {e:?}"))?;
    Ok(())
}

pub fn make(factory: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory)
        .build()
        .map_err(|_| format!("GStreamer has no {factory} element here. {}", gmx_netkit::elements::where_from(factory)))
}
