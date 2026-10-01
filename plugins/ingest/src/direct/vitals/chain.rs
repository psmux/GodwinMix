//! A decoder driven by hand, one job at a time, shared by every show.
//!
//! A keyframe needs nothing before it, so one decoder can take a keyframe
//! from one show and the next from another. What it cannot do on its own is
//! hand the picture back at once: a decoder that expects B frames holds each
//! picture until the next arrives. So every job ends with an end of stream,
//! which drains the decoder, and a flush, which makes it ready for the next
//! job. The caller's thread pushes the buffers through a pad of our own, so
//! decoding, scaling and the sink all run in that call and the samples are
//! waiting in the sink when it returns. There is no queue and no streaming
//! thread of GStreamer's own anywhere in this.
//!
//! ```text
//!   our pad ──► decoder ──► convert ──► scale ──► caps ──► appsink
//!   stream-start, caps (when they change), segment, buffers, EOS, flush
//! ```

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;

pub struct Chain {
    pipeline: gst::Pipeline,
    pad: gst::Pad,
    sink: AppSink,
    caps: Option<gst::Caps>,
    started: bool,
}

fn make(factory: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory)
        .build()
        .map_err(|_| format!("the GStreamer element {factory} is missing; install the plugin set that has it"))
}

impl Chain {
    /// `names` in a row (a parser, a decoder, a scaler...), then `out` as a
    /// caps filter, then a sink.
    pub fn new(names: &[&str], out: gst::Caps) -> Result<Chain, String> {
        let pipeline = gst::Pipeline::new();
        // Nobody reads this bus: a refused keyframe's error is answered by
        // building the chain again, so its messages are dropped, not kept.
        if let Some(bus) = pipeline.bus() {
            bus.set_flushing(true);
        }
        let mut elements = Vec::new();
        for name in names {
            let e = make(name)?;
            if e.has_property("max-threads") {
                // One thread: a thread pool per decoder is memory for nothing
                // when it decodes one picture a second.
                e.set_property("max-threads", 1i32);
            }
            elements.push(e);
        }
        let filter = make("capsfilter")?;
        filter.set_property("caps", &out);
        elements.push(filter);
        let sink = AppSink::builder().sync(false).build();
        sink.set_property("async", false);
        // The end of stream is pushed from the same thread that pulls the
        // samples afterwards, so the sink must not wait for them to be
        // pulled before it takes the end.
        sink.set_property("wait-on-eos", false);
        pipeline.add_many(&elements).map_err(|e| e.to_string())?;
        pipeline.add(&sink).map_err(|e| e.to_string())?;
        gst::Element::link_many(&elements).map_err(|e| e.to_string())?;
        elements.last().unwrap().link(&sink).map_err(|e| e.to_string())?;
        let pad = gst::Pad::builder(gst::PadDirection::Src).name("vitals").build();
        let sinkpad = elements[0].static_pad("sink").ok_or("the decoder has no sink pad")?;
        pad.link(&sinkpad).map_err(|e| format!("{e:?}"))?;
        pad.set_active(true).map_err(|e| e.to_string())?;
        pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
        Ok(Chain { pipeline, pad, sink, caps: None, started: false })
    }

    /// Decode `buffers` under `caps` and hand back every sample that came
    /// out, oldest first. Empty when the decoder refused them.
    pub fn run(&mut self, caps: &gst::Caps, buffers: Vec<gst::Buffer>) -> Vec<gst::Sample> {
        if !self.started {
            self.pad.push_event(gst::event::StreamStart::new("vitals"));
            self.started = true;
        }
        if self.caps.as_ref() != Some(caps) {
            self.pad.push_event(gst::event::Caps::new(caps));
            self.caps = Some(caps.clone());
        }
        let segment = gst::FormattedSegment::<gst::ClockTime>::new();
        self.pad.push_event(gst::event::Segment::new(&segment));
        for b in buffers {
            if self.pad.push(b).is_err() {
                break;
            }
        }
        self.pad.push_event(gst::event::Eos::new());
        let mut out = Vec::new();
        while let Some(s) = self.sink.try_pull_sample(gst::ClockTime::ZERO) {
            out.push(s);
        }
        self.pad.push_event(gst::event::FlushStart::new());
        self.pad.push_event(gst::event::FlushStop::new(true));
        out
    }
}

impl Drop for Chain {
    fn drop(&mut self) {
        let _ = self.pad.set_active(false);
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
