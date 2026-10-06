//! A picture that changes size mid stream, through the remuxer and out the
//! other side of a decoder. A phone turned upright does exactly this, and
//! so does a browser that starts small and climbs as it finds bandwidth.
//!
//! The parameter sets are taken out of the frames first, so the only place
//! the new size is written is the second sequence header. That is the worst
//! a publisher can send, and it is what a WebRTC depayloader may hand on.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;

use super::{Out, Remux};
use crate::media_tag::{MediaTag, TagKind};

/// A frame's NAL units without its SPS (7) and PPS (8). AVC framing: each
/// unit has a four byte length in front. The five byte FLV prefix stays.
fn without_parameter_sets(tag: &MediaTag) -> MediaTag {
    if tag.kind != TagKind::Video || tag.sequence_header || tag.payload.len() < 5 {
        return tag.clone();
    }
    let (head, mut rest) = tag.payload.split_at(5);
    let mut out = head.to_vec();
    while rest.len() >= 4 {
        let len = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        let Some(unit) = rest.get(4..4 + len) else { break };
        if !matches!(unit.first().map(|b| b & 0x1f), Some(7 | 8)) {
            out.extend_from_slice(&rest[..4 + len]);
        }
        rest = &rest[4 + len..];
    }
    MediaTag { payload: Arc::from(out), ..tag.clone() }
}

/// Decode a Matroska file and count the pictures of each width.
fn widths(path: &std::path::Path) -> Vec<(i32, u32)> {
    let line = format!("filesrc location=\"{}\" ! matroskademux ! h264parse ! avdec_h264 ! fakesink name=s sync=false", path.display().to_string().replace('\\', "/"));
    let p = gst::parse::launch(&line).unwrap().downcast::<gst::Pipeline>().unwrap();
    let seen: Arc<Mutex<Vec<(i32, u32)>>> = Arc::default();
    let s = seen.clone();
    p.by_name("s").unwrap().static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |pad, _| {
        let width = pad.current_caps().and_then(|c| c.structure(0).and_then(|st| st.get::<i32>("width").ok())).unwrap_or(0);
        let mut s = s.lock().unwrap();
        match s.last_mut() {
            Some((w, n)) if *w == width => *n += 1,
            _ => s.push((width, 1)),
        }
        gst::PadProbeReturn::Ok
    });
    p.set_state(gst::State::Playing).unwrap();
    let _ = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(20), &[gst::MessageType::Eos, gst::MessageType::Error]);
    let _ = p.set_state(gst::State::Null);
    let out = seen.lock().unwrap().clone();
    out
}

#[test]
fn a_picture_that_changes_size_mid_stream_keeps_decoding_at_the_new_size() {
    gmx_netkit::init().unwrap();
    if !gmx_netkit::elements::missing(&["x264enc", "avenc_aac", "avdec_h264"]).is_empty() {
        eprintln!("skipped: needs x264enc, avenc_aac and avdec_h264");
        return;
    }
    let wide = crate::testfeed::sized(45, 320, 240);
    let upright = crate::testfeed::sized(45, 240, 320);
    let shift = wide.iter().map(|t| t.timestamp_ms).max().unwrap_or(0) + 33;
    let path = std::env::temp_dir().join(format!("gmx-remux-size-{}.mkv", std::process::id()));
    let remux = Remux::open(Out::File(path.clone()), None).expect("the remuxer assembles");
    remux.write(&crate::flv::header());
    let later = upright.into_iter().map(|t| MediaTag { timestamp_ms: t.timestamp_ms + shift, ..t });
    for tag in wide.into_iter().chain(later) {
        remux.write(&crate::flv::write(&without_parameter_sets(&tag)));
    }
    // Dropping the remuxer sends end of stream and stops it at once, so the
    // file holds what it got through by then. Half a second was enough here
    // and on Linux; a loaded Windows runner wrote 11 of the 45 upright
    // pictures, so the wait is longer by GODWINMIX_TIMING_SLACK, and two
    // seconds rather than half of one: one and a half (half times the slack) did
    // not do either.
    let slack = std::env::var("GODWINMIX_TIMING_SLACK").ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0).max(1.0);
    std::thread::sleep(std::time::Duration::from_secs(2).mul_f64(slack));
    drop(remux);
    let seen = widths(&path);
    let _ = std::fs::remove_file(&path);
    let upright_frames: u32 = seen.iter().filter(|(w, _)| *w == 240).map(|(_, n)| n).sum();
    assert!(seen.iter().any(|(w, _)| *w == 320), "the first size never decoded: {seen:?}");
    assert!(upright_frames >= 40, "after the change only {upright_frames} pictures decoded at the new size: {seen:?}");
}
