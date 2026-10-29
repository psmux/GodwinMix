//! A scale node fed pictures of a stated pixel shape and scan, on the CPU,
//! as the graph builds it: a 16:9 picture stored at 704x480 fills a 16:9
//! frame, a 4:3 one gets bars, an override beats the stream, a shape that
//! changes mid stream changes the fit, and a deinterlace takes the comb off
//! a moving interlaced picture. The GPU path builds the same elements around
//! `cudaconvertscale` and is checked on a machine with a card.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use serde_json::{json, Value};

use super::build::build;
use super::spec::NodeSpec;

const BLACK: u8 = 16;
const WHITE: u8 = 235;

pub(super) fn node(extra: Value) -> NodeSpec {
    let mut raw = json!({"kind": "scale", "width": 320, "height": 180, "fps": [30, 1], "fit": "pad", "border": 0xff00_0000u64, "deinterlace": "off", "rate": false});
    for (k, v) in extra.as_object().unwrap() {
        raw[k] = v.clone();
    }
    NodeSpec { id: "scale:pic".into(), kind: "scale".into(), input: "decode".into(), raw }
}

/// An I420 picture whose luma at (x, y) is `luma(x, y)`, chroma grey,
/// flagged as two fields top first when `fields`, as a decoder flags each
/// picture of an interlaced sequence.
fn picture(w: usize, h: usize, pts_ms: u64, fields: bool, luma: impl Fn(usize, usize) -> u8) -> gst::Buffer {
    let mut data = vec![128u8; w * h * 3 / 2];
    for y in 0..h {
        for x in 0..w {
            data[y * w + x] = luma(x, y);
        }
    }
    let mut b = gst::Buffer::from_mut_slice(data);
    let m = b.get_mut().unwrap();
    m.set_pts(gst::ClockTime::from_mseconds(pts_ms));
    m.set_duration(gst::ClockTime::from_mseconds(33));
    if fields {
        // GstVideoBufferFlags INTERLACED and TFF, the first two after
        // GstBufferFlags' own.
        m.set_flags(gst::BufferFlags::from_bits_retain(0x10_0000 | 0x20_0000));
    }
    b
}

/// What comes out of the node: each picture's luma plane and size.
pub(super) struct Out {
    pub(super) frames: Vec<(usize, usize, Vec<u8>)>,
}

impl Out {
    fn luma(&self, i: usize, x: usize, y: usize) -> u8 {
        let (w, _, l) = &self.frames[i];
        l[y * w + x]
    }
}

/// One run of pictures under one caps: `count` of them at `width` x
/// `height`, flagged as fields or not, picture `k`'s luma `luma(k, x, y)`.
pub(super) struct Run<'a> {
    pub(super) caps: &'a str,
    pub(super) size: (usize, usize),
    pub(super) count: usize,
    pub(super) fields: bool,
    pub(super) luma: &'a dyn Fn(usize, usize, usize) -> u8,
}

fn plain<'a>(caps: &'a str, size: (usize, usize), luma: &'a dyn Fn(usize, usize, usize) -> u8) -> Run<'a> {
    Run { caps, size, count: 4, fields: false, luma }
}

/// Push `runs` through the node, frames 33 ms apart.
fn run(n: &NodeSpec, runs: &[Run]) -> Out {
    run_with(n, runs, false)
}

/// The same, on the card when `cuda`: up to CUDA memory before the node
/// and back down after it, as the decoder and the encoder would hand over.
pub(super) fn run_with(n: &NodeSpec, runs: &[Run], cuda: bool) -> Out {
    gmx_netkit::init().unwrap();
    let built = build(n).unwrap_or_else(|e| panic!("{}: {e}", n.id));
    let pipeline = gst::Pipeline::new();
    let src = AppSrc::builder().format(gst::Format::Time).build();
    let sink = gst::ElementFactory::make("appsink").build().unwrap().downcast::<AppSink>().unwrap();
    sink.set_caps(Some(&"video/x-raw,format=I420".parse().unwrap()));
    sink.set_sync(false);
    sink.set_max_buffers(0);
    let convert = gst::ElementFactory::make("videoconvert").build().unwrap();
    let ends = if cuda { ["cudaupload", "cudadownload"] } else { ["identity", "identity"] };
    let (up, down) = (gst::ElementFactory::make(ends[0]).build().unwrap(), gst::ElementFactory::make(ends[1]).build().unwrap());
    if cuda {
        keep_flags(&up);
    }
    pipeline.add_many([src.upcast_ref(), &up, &down]).unwrap();
    pipeline.add_many(built.elements.iter()).unwrap();
    pipeline.add_many([&convert, sink.upcast_ref()]).unwrap();
    gst::Element::link_many(built.elements.iter()).unwrap();
    gst::Element::link_many([src.upcast_ref(), &up, built.head.as_ref().unwrap()]).unwrap();
    gst::Element::link_many([built.tee.as_ref().unwrap(), &down, &convert, sink.upcast_ref()]).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let mut at = 0u64;
    let mut sent = 0;
    for r in runs {
        src.set_caps(Some(&r.caps.parse::<gst::Caps>().unwrap()));
        for k in 0..r.count {
            src.push_buffer(picture(r.size.0, r.size.1, at, r.fields, |x, y| (r.luma)(k, x, y))).unwrap();
            at += 33;
            sent += 1;
        }
    }
    src.end_of_stream().unwrap();
    let mut frames = Vec::new();
    while let Some(s) = sink.try_pull_sample(gst::ClockTime::from_seconds(10)) {
        let st = s.caps().unwrap().structure(0).unwrap().to_owned();
        let (w, h) = (st.get::<i32>("width").unwrap() as usize, st.get::<i32>("height").unwrap() as usize);
        let map = s.buffer().unwrap().map_readable().unwrap();
        frames.push((w, h, map[..w * h].to_vec()));
        if frames.len() == sent {
            break;
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    assert!(!frames.is_empty(), "{}: no picture came out", n.id);
    Out { frames }
}

/// `cudaupload` hands a picture on without its flags, where a decoder's
/// CUDA pictures keep theirs: each one's put back on the way out.
fn keep_flags(up: &gst::Element) {
    let flags = std::sync::Arc::new(std::sync::Mutex::new(0u32));
    let noted = flags.clone();
    up.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        if let Some(b) = info.buffer() {
            *noted.lock().unwrap() = super::cuda_deint::raw_flags(b);
        }
        gst::PadProbeReturn::Ok
    });
    up.static_pad("src").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        let f = *flags.lock().unwrap();
        if let Some(b) = info.buffer_mut() {
            // SAFETY: `make_mut` gives a buffer of our own to write.
            unsafe { (*b.make_mut().as_mut_ptr()).mini_object.flags |= f }
        }
        gst::PadProbeReturn::Ok
    });
}

fn white(_: usize, _: usize, _: usize) -> u8 {
    WHITE
}

const WIDE_SD: &str = "video/x-raw,format=I420,width=704,height=480,framerate=30000/1001,pixel-aspect-ratio=40/33";
const SQUARE_SD: &str = "video/x-raw,format=I420,width=640,height=480,framerate=30000/1001,pixel-aspect-ratio=1/1";

#[test]
fn a_wide_picture_stored_at_704x480_fills_a_wide_frame() {
    let out = run(&node(json!({})), &[plain(WIDE_SD, (704, 480), &white)]);
    assert_eq!((out.frames[0].0, out.frames[0].1), (320, 180));
    assert!(out.luma(0, 2, 90) > 200, "the left edge is picture, not a bar: luma {}", out.luma(0, 2, 90));
}

#[test]
fn a_square_4_3_picture_is_laid_in_with_bars() {
    let out = run(&node(json!({})), &[plain(SQUARE_SD, (640, 480), &white)]);
    // 4:3 in 320x180 is 240 wide: 40 of bar either side.
    assert!(out.luma(0, 20, 90) < 40, "a pillar bar at x 20: luma {}", out.luma(0, 20, 90));
    assert!(out.luma(0, 60, 90) > 200, "the picture at x 60: luma {}", out.luma(0, 60, 90));
}

#[test]
fn an_override_beats_what_the_stream_says() {
    let standard = run(&node(json!({"aspect": [4, 3]})), &[plain(WIDE_SD, (704, 480), &white)]);
    assert!(standard.luma(0, 20, 90) < 40, "forced 4:3 has bars: luma {}", standard.luma(0, 20, 90));
    let wide = run(&node(json!({"aspect": [16, 9]})), &[plain(SQUARE_SD, (640, 480), &white)]);
    assert!(wide.luma(0, 2, 90) > 200, "forced 16:9 fills: luma {}", wide.luma(0, 2, 90));
}

#[test]
fn crop_cuts_by_the_shape_shown_not_the_shape_stored() {
    // The top 40 rows are black. Shown at 16:9 the picture is already the
    // frame's shape and nothing is cut, so the top row stays black; taken
    // as square pixels it would have lost its top and bottom.
    let banded = |_: usize, _: usize, y: usize| if y < 40 { BLACK } else { WHITE };
    let out = run(&node(json!({"fit": "crop"})), &[plain(WIDE_SD, (704, 480), &banded)]);
    assert!(out.luma(0, 160, 2) < 40, "the black band is still there: luma {}", out.luma(0, 160, 2));
}

#[test]
fn a_shape_that_changes_mid_stream_changes_the_fit() {
    let standard = "video/x-raw,format=I420,width=704,height=480,framerate=30000/1001,pixel-aspect-ratio=10/11";
    let out = run(&node(json!({})), &[plain(WIDE_SD, (704, 480), &white), plain(standard, (704, 480), &white)]);
    let last = out.frames.len() - 1;
    assert!(out.luma(0, 2, 90) > 200, "16:9 first, filling: luma {}", out.luma(0, 2, 90));
    assert!(out.luma(last, 20, 90) < 40, "then 4:3, with bars: luma {}", out.luma(last, 20, 90));
}

/// A white bar moving right 8 pixels a frame on black, each frame's two
/// fields taken half a frame apart, as an interlaced camera sees it: the
/// bar's edges comb.
pub(super) fn moving(k: usize, x: usize, y: usize) -> u8 {
    let left = 40 + 8 * k + if y % 2 == 1 { 4 } else { 0 };
    if (left..left + 60).contains(&x) {
        WHITE
    } else {
        BLACK
    }
}

/// How much neighbouring lines differ around the bar: high for a comb.
pub(super) fn comb(out: &Out, i: usize) -> u32 {
    let (w, h, l) = &out.frames[i];
    let mut sum = 0u32;
    for y in (20..h - 20).step_by(2) {
        for x in 0..*w {
            sum += u32::from(l[y * w + x].abs_diff(l[(y + 1) * w + x]));
        }
    }
    sum
}

#[test]
fn a_deinterlace_takes_the_comb_off_a_moving_interlaced_picture() {
    // As NVDEC and libav hand on a broadcast's pictures: `mixed`, and each
    // picture flagged as two fields.
    let mixed = "video/x-raw,format=I420,width=320,height=240,framerate=30000/1001,pixel-aspect-ratio=1/1,interlace-mode=mixed";
    let fields = || [Run { caps: mixed, size: (320, 240), count: 8, fields: true, luma: &moving }];
    let woven = run(&node(json!({"width": 320, "height": 240, "deinterlace": "off"})), &fields());
    let last = woven.frames.len() - 1;
    let before = comb(&woven, last);
    assert!(before > 10_000, "woven, the bar's edges comb: {before}");
    for (mode, method) in [("on", "yadif"), ("auto", "yadif"), ("on", "greedyh"), ("on", "linear")] {
        let out = run(&node(json!({"width": 320, "height": 240, "deinterlace": mode, "method": method})), &fields());
        let after = comb(&out, out.frames.len() - 1);
        assert!(after * 3 < before, "deinterlace {mode} with {method} leaves a comb: {after} against {before} woven");
    }
    // A picture of a mixed stream that is not flagged as fields, a film
    // coded as frames, is left as it is by `auto`.
    let frames = [Run { caps: mixed, size: (320, 240), count: 8, fields: false, luma: &moving }];
    let kept = run(&node(json!({"width": 320, "height": 240, "deinterlace": "auto"})), &frames);
    assert_eq!(comb(&kept, kept.frames.len() - 1), before, "auto deinterlaced a picture not flagged as fields");
}
