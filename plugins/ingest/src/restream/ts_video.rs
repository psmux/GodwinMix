//! The video half of an SRT copy: flvdemux's video pad, through the parser
//! its codec needs, into the MPEG-TS muxer. Chosen when the pad appears,
//! because the same destination carries H.264 from one publisher and enhanced
//! RTMP HEVC from the next.

use gstreamer as gst;
use gstreamer::prelude::*;

/// Link the demuxer called `d`'s video pad, when it comes, to the muxer
/// called `mux`, through `h264parse` or `h265parse` with parameter sets
/// before every keyframe, so a receiver that joins late starts at the next.
pub fn video_by_codec(pipeline: &gst::Pipeline) {
    let (Some(demux), Some(mux)) = (pipeline.by_name("d"), pipeline.by_name("mux")) else { return };
    let weak = pipeline.downgrade();
    demux.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        if !pad.name().starts_with("video") {
            return;
        }
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let hevc = caps.structure(0).is_some_and(|s| s.name() == "video/x-h265");
        let _ = link(&pipeline, pad, &mux, if hevc { "h265parse" } else { "h264parse" });
    });
}

fn link(pipeline: &gst::Pipeline, pad: &gst::Pad, mux: &gst::Element, parser: &str) -> Option<()> {
    let queue = gst::ElementFactory::make("queue").build().ok()?;
    let parse = gst::ElementFactory::make(parser).property("config-interval", -1i32).build().ok()?;
    pipeline.add_many([&queue, &parse]).ok()?;
    queue.link(&parse).ok()?;
    let target = mux.request_pad_simple("sink_%d")?;
    parse.static_pad("src")?.link(&target).ok()?;
    for e in [&queue, &parse] {
        let _ = e.sync_state_with_parent();
    }
    pad.link(&queue.static_pad("sink")?).ok()?;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_tag::{MediaTag, TagKind};
    use crate::rtmp::Inlet;
    use std::sync::{Arc, Mutex};

    struct Keep(Arc<Mutex<Vec<MediaTag>>>);
    impl Inlet for Keep {
        fn tag(&mut self, tag: MediaTag) {
            self.0.lock().unwrap().push(tag);
        }
    }

    /// Two seconds of HEVC as this plugin frames it on the hub, as FLV bytes.
    fn hevc_flv() -> Vec<u8> {
        let got = Arc::new(Mutex::new(Vec::new()));
        let to = crate::tagger::share(Box::new(Keep(got.clone())));
        let p = gst::parse::launch(
            "videotestsrc num-buffers=60 ! video/x-raw,format=I420,width=320,height=240,framerate=30/1 \
             ! x265enc speed-preset=ultrafast key-int-max=30 ! h265parse name=vp",
        )
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
        let sink = crate::tagger::hevc_sink(to, Arc::new(crate::tagger::Zero::default()));
        p.add(&sink).unwrap();
        p.by_name("vp").unwrap().link(&sink).unwrap();
        p.set_state(gst::State::Playing).unwrap();
        let _ = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(20), &[gst::MessageType::Eos, gst::MessageType::Error]);
        let _ = p.set_state(gst::State::Null);
        let mut out = crate::flv::header();
        for t in got.lock().unwrap().iter().filter(|t| t.kind == TagKind::Video) {
            out.extend(crate::flv::video(t.timestamp_ms, &t.payload));
        }
        out
    }

    #[test]
    fn enhanced_rtmp_hevc_on_the_hub_becomes_hevc_in_mpeg_ts() {
        gmx_netkit::init().unwrap();
        if gst::ElementFactory::find("x265enc").is_none() {
            eprintln!("skipping: needs x265enc");
            return;
        }
        let dir = std::env::temp_dir().join(format!("gmx-ts-hevc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (flv, ts) = (dir.join("in.flv"), dir.join("out.ts"));
        std::fs::write(&flv, hevc_flv()).unwrap();
        let line = format!("filesrc location={} ! flvdemux name=d mpegtsmux name=mux ! filesink location={}", flv.display(), ts.display());
        let p = gst::parse::launch(&line).unwrap().downcast::<gst::Pipeline>().unwrap();
        video_by_codec(&p);
        p.set_state(gst::State::Playing).unwrap();
        let _ = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Eos, gst::MessageType::Error]);
        let _ = p.set_state(gst::State::Null);
        let check = format!("filesrc location={} ! tsdemux ! h265parse ! avdec_h265 ! fakesink name=end", ts.display());
        let c = gst::parse::launch(&check).unwrap().downcast::<gst::Pipeline>().unwrap();
        let frames = Arc::new(Mutex::new(0u32));
        let f = frames.clone();
        c.by_name("end").unwrap().static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            *f.lock().unwrap() += 1;
            gst::PadProbeReturn::Ok
        });
        c.set_state(gst::State::Playing).unwrap();
        let _ = c.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Eos, gst::MessageType::Error]);
        let _ = c.set_state(gst::State::Null);
        let _ = std::fs::remove_dir_all(&dir);
        let n = *frames.lock().unwrap();
        assert!(n >= 55, "decoded {n} HEVC frames of 60 out of the MPEG-TS");
    }
}
