//! The tags of the native path against the GStreamer path's, for the same
//! file: payloads and flags byte for byte, times within a millisecond.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;

use super::feed;
use crate::direct::input::pads::{parse_into, Pads};
use crate::direct::input::spec::Carry;
use crate::direct::input::tests::{clip, which};
use crate::media_tag::{MediaTag, TagKind};
use crate::rtmp::Inlet;

struct Keep(Arc<Mutex<Vec<MediaTag>>>);
impl Inlet for Keep {
    fn tag(&mut self, tag: MediaTag) {
        self.0.lock().unwrap().push(tag);
    }
}

/// The tags `filesrc ! parsebin ! h264parse | aacparse ! appsink` make.
pub fn gstreamer_tags(path: &std::path::Path) -> Vec<MediaTag> {
    gmx_netkit::init().unwrap();
    let got = Arc::new(Mutex::new(Vec::new()));
    let to = crate::tagger::share(Box::new(Keep(got.clone())));
    let pipeline = gst::Pipeline::new();
    let src = gst::ElementFactory::make("filesrc").property("location", path.to_str().unwrap()).build().unwrap();
    pipeline.add(&src).unwrap();
    let pads = Pads::new(to.clone(), false, None);
    parse_into(&pipeline, &src.static_pad("src").unwrap(), &pads).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let bus = pipeline.bus().unwrap();
    let msg = bus.timed_pop_filtered(gst::ClockTime::from_seconds(60), &[gst::MessageType::Eos, gst::MessageType::Error]);
    assert!(matches!(msg.as_ref().map(|m| m.view()), Some(gst::MessageView::Eos(_))), "{msg:?}");
    let _ = pipeline.set_state(gst::State::Null);
    crate::tagger::close(&to);
    let tags = got.lock().unwrap().clone();
    tags
}

type Line = Vec<(i64, bool, bool, Arc<[u8]>)>;

/// One kind's tags as `(time, keyframe, header, payload)`.
fn line(tags: &[MediaTag], kind: TagKind) -> Line {
    tags.iter().filter(|t| t.kind == kind && t.track == 0).map(|t| (i64::from(t.timestamp_ms), t.keyframe, t.sequence_header, t.payload.clone())).collect()
}

/// Both lines from the first frame GStreamer handed on, headers apart, with
/// times from there. `filesrc ! parsebin` loses the first frame or two while
/// it links its pads, and the native path has no last frame until the next
/// packet starts, which at the end of a file never comes; both ends are
/// trimmed.
fn aligned(a: Line, b: Line) -> (Line, Line) {
    let (head_a, mut a): (Line, Line) = a.into_iter().partition(|x| x.2);
    let (head_b, mut b): (Line, Line) = b.into_iter().partition(|x| x.2);
    let bodies = |h: &Line| h.iter().map(|x| x.3.clone()).collect::<Vec<_>>();
    assert!(bodies(&head_a) == bodies(&head_b), "sequence headers: {:02x?} against {:02x?}", bodies(&head_a), bodies(&head_b));
    let mine = b.iter().position(|y| y.3 == a[0].3).expect("GStreamer's first frame is among the native ones");
    b.drain(..mine);
    let n = a.len().min(b.len());
    assert!(a.len() - n <= 1 && b.len() - n <= 1, "{} against {} tags", a.len(), b.len());
    a.truncate(n);
    b.truncate(n);
    // Times from the second frame: GStreamer's first one after a start in
    // the middle of a GOP has a decode time its parser guessed (27 ms late
    // on an HEVC clip with B frames), where the native path reads it.
    for l in [&mut a, &mut b] {
        let z = l[1.min(l.len() - 1)].0;
        l.iter_mut().for_each(|x| x.0 -= z);
    }
    (a, b)
}

pub fn assert_same(path: &std::path::Path, what: &str) {
    let theirs = gstreamer_tags(path);
    let ours = feed::tags(&std::fs::read(path).unwrap(), Carry::default());
    for kind in [TagKind::Video, TagKind::Audio] {
        let (a, b) = aligned(line(&theirs, kind), line(&ours, kind));
        if std::env::var("NATIVE_DUMP").is_ok() {
            for (i, (x, y)) in a.iter().zip(&b).enumerate().take(6) {
                eprintln!("{kind:?} {i}: gst {} {} {} {} {:02x?} | native {} {} {} {} {:02x?}", x.0, x.1, x.2, x.3.len(), &x.3[..x.3.len().min(16)], y.0, y.1, y.2, y.3.len(), &y.3[..y.3.len().min(16)]);
            }
            eprintln!("{kind:?} last: gst {:?} native {:?}", a.last().map(|x| (x.0, x.3.len())), b.last().map(|y| (y.0, y.3.len())));
        }
        assert!(!a.is_empty(), "{what}: GStreamer made no {kind:?} tags");
        assert_eq!(a.len(), b.len(), "{what}: {kind:?} tags, GStreamer against native");
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            let at = format!("{what}: {kind:?} tag {i}");
            assert_eq!((x.1, x.2), (y.1, y.2), "{at}: keyframe and header flags");
            assert!(x.3 == y.3, "{at}: {} bytes against {}; first difference at {:?}; {:02x?} against {:02x?}", x.3.len(), y.3.len(),
                x.3.iter().zip(y.3.iter()).position(|(p, q)| p != q), &x.3[..x.3.len().min(24)], &y.3[..y.3.len().min(24)]);
            assert!(i == 0 || (x.0 - y.0).abs() <= 1, "{at}: {} ms against {} ms", x.0, y.0);
        }
    }
}

#[test]
fn h264_and_aac_tags_are_byte_for_byte_the_gstreamer_paths() {
    if !which("ffmpeg") {
        return;
    }
    // 720p with B frames, so the composition offsets are not all zero, and
    // two sound tracks, of which both paths carry the first.
    let path = clip("native-h264-aac.ts", &["-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30000/1001", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
        "-f", "lavfi", "-i", "sine=frequency=660:sample_rate=48000", "-map", "0", "-map", "1", "-map", "2", "-t", "6",
        "-c:v", "libx264", "-preset", "veryfast", "-g", "30", "-bf", "2", "-c:a", "aac", "-ac", "2", "-f", "mpegts"]);
    assert_same(&path, "h264 and aac");
}

#[test]
fn hevc_and_ac3_tags_are_byte_for_byte_the_gstreamer_paths() {
    if !which("ffmpeg") {
        return;
    }
    let path = clip("native-hevc-ac3.ts", &["-f", "lavfi", "-i", "testsrc2=size=640x360:rate=25", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
        "-t", "4", "-c:v", "libx265", "-preset", "ultrafast", "-x265-params", "keyint=25:bframes=2:log-level=error", "-c:a", "ac3", "-ac", "6", "-f", "mpegts"]);
    assert_same(&path, "hevc and ac3");
}

#[test]
fn mpeg_layer_two_tags_are_byte_for_byte_the_gstreamer_paths() {
    if !which("ffmpeg") {
        return;
    }
    let path = clip("native-h264-mp2.ts", &["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
        "-t", "4", "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-c:a", "mp2", "-b:a", "192k", "-f", "mpegts"]);
    assert_same(&path, "h264 and mp2");
}
