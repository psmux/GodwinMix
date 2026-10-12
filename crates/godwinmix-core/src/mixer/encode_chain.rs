//! What the programme's two encode chains are made of, as recipes the
//! encoder runs on its first consumer. See `encoder::lazy` for why they are
//! not made at boot.

use super::{encode_bridge, parser_for};
use crate::catalogue::apply::{self, Vars};
use crate::catalogue::Selection;
use crate::encoder::Build;
use crate::gstutil::{self, make};
use gstreamer as gst;
use gstreamer::prelude::*;

/// Queue, whatever it takes to get the canvas into the shape this encoder
/// wants, the encoder, its parser. With the software graphics entry the
/// bridge is one `videoconvert`; with a GPU entry the frame has been on the
/// GPU since the compositor and comes down only if the encoder cannot take it
/// there.
///
/// `av_offset_ns` holds the video back by the audio encoder's delay, at the
/// encoder's own pad, so the raw programme tee is never shifted.
pub(super) fn video(sel: &Selection, vars: Vars, av_offset_ns: i64) -> Build {
    let (gfx, enc) = (sel.graphics.clone(), sel.video_encode.clone());
    Box::new(move || {
        let venc = make(&enc.element, "venc")?;
        apply::apply(&venc, &enc.properties, &vars);
        apply::apply_keyframe(&venc, enc.keyframe.as_ref(), &vars);
        if let Some(sink) = venc.static_pad("sink") {
            sink.set_offset(av_offset_ns);
        }
        let mut chain: Vec<gst::Element> = vec![gstutil::queue_thread("venc-q")?];
        chain.extend(encode_bridge(&gfx, &enc)?);
        chain.push(venc);
        chain.extend(parser_for(enc.parser.as_deref(), "vparse")?);
        Ok(chain)
    })
}

/// Queue, conversion, `audiorate`, the encoder, its parser.
///
/// `audiorate` is defensive: the mixer is a live aggregator whose inputs are
/// stamped by other processes against their own clocks, so a contiguous
/// stream into the encoder is guaranteed rather than assumed. Added while
/// chasing an RTMP server that re-served AAC with its timeline stepped
/// backwards; kept because it costs nothing.
///
/// `skip-to-first`, because the chain is attached to a programme that may
/// have been running for hours. Without it `audiorate` fills from the start
/// of the segment to its first buffer, and the first output added an hour in
/// got an hour of silence in one burst, stamped from zero. Found on
/// 2026-10-01 by the scale harness: a udp output's remuxer waited on that
/// audio for its video and never sent again.
pub(super) fn audio(sel: &Selection, vars: Vars) -> Build {
    let enc = sel.audio_encode.clone();
    Box::new(move || {
        let aenc = make(&enc.element, "aenc")?;
        apply::apply(&aenc, &enc.properties, &vars);
        let arate = make("audiorate", "aenc-rate")?;
        crate::probe::set_bool(&arate, "skip-to-first", true);
        let mut chain = vec![
            gstutil::queue_thread("aenc-q")?,
            make("audioconvert", "aenc-conv")?,
            arate,
            aenc,
        ];
        chain.extend(parser_for(enc.parser.as_deref(), "aparse")?);
        Ok(chain)
    })
}
