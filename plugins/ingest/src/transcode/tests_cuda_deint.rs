//! The deinterlace on the card (`cuda_deint.rs`), through a scale node in
//! CUDA memory as the graph builds it. Needs an NVIDIA card, its driver,
//! GStreamer's nvcodec plugin and NVRTC, so it is ignored by default:
//!
//!   cargo test -p gmx-ingest cuda_deint -- --ignored

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use gstreamer_video::VideoFrameExt;
use serde_json::{json, Value};

use super::build::build;
use super::spec::NodeSpec;

const BLACK: u8 = 16;
const WHITE: u8 = 235;
const W: usize = 320;
const H: usize = 240;
const MIXED: &str = "video/x-raw,format=NV12,width=320,height=240,framerate=30000/1001,pixel-aspect-ratio=1/1,interlace-mode=mixed";

fn node(extra: Value) -> NodeSpec {
    let mut raw = json!({"kind": "scale", "width": W, "height": H, "fps": [30, 1], "fit": "pad", "border": 0xff00_0000u64, "rate": false, "memory": "cuda"});
    for (k, v) in extra.as_object().unwrap() {
        raw[k] = v.clone();
    }
    NodeSpec { id: "scale:cuda".into(), kind: "scale".into(), input: "decode".into(), raw }
}

/// NV12 with grey chroma, flagged INTERLACED and TFF when `fields`.
fn picture(pts_ms: u64, fields: bool, luma: impl Fn(usize, usize) -> u8) -> gst::Buffer {
    let mut data = vec![128u8; W * H * 3 / 2];
    for y in 0..H {
        for x in 0..W {
            data[y * W + x] = luma(x, y);
        }
    }
    let mut b = gst::Buffer::from_mut_slice(data);
    let m = b.get_mut().unwrap();
    m.set_pts(gst::ClockTime::from_mseconds(pts_ms));
    m.set_duration(gst::ClockTime::from_mseconds(33));
    if fields {
        m.set_flags(gst::BufferFlags::from_bits_retain(0x10_0000 | 0x20_0000));
    }
    b
}

/// Each output picture's luma plane.
fn run(n: &NodeSpec, count: usize, fields: bool, luma: &dyn Fn(usize, usize, usize) -> u8) -> Vec<Vec<u8>> {
    gmx_netkit::init().unwrap();
    let built = build(n).unwrap_or_else(|e| panic!("{}: {e}", n.id));
    let pipeline = gst::Pipeline::new();
    super::cuda::share(&pipeline);
    let src = AppSrc::builder().format(gst::Format::Time).caps(&MIXED.parse().unwrap()).build();
    let up = gst::ElementFactory::make("cudaupload").build().expect("cudaupload: is nvcodec installed and a card present?");
    // cudaupload makes new buffers without the picture's field flags; a
    // decoder sets them on the CUDA buffer itself, so put them back here.
    if fields {
        up.static_pad("src").unwrap().add_probe(gst::PadProbeType::BUFFER, |_, info| {
            if let Some(gst::PadProbeData::Buffer(b)) = &mut info.data {
                b.make_mut().set_flags(gst::BufferFlags::from_bits_retain(0x10_0000 | 0x20_0000));
            }
            gst::PadProbeReturn::Ok
        });
    }
    let down = gst::ElementFactory::make("cudadownload").build().unwrap();
    let sink = gst::ElementFactory::make("appsink").build().unwrap().downcast::<AppSink>().unwrap();
    sink.set_caps(Some(&"video/x-raw,format=NV12".parse().unwrap()));
    sink.set_sync(false);
    sink.set_max_buffers(0);
    pipeline.add_many([src.upcast_ref(), &up]).unwrap();
    pipeline.add_many(built.elements.iter()).unwrap();
    pipeline.add_many([&down, sink.upcast_ref()]).unwrap();
    gst::Element::link_many([src.upcast_ref(), &up]).unwrap();
    up.link_filtered(built.head.as_ref().unwrap(), &"video/x-raw(memory:CUDAMemory)".parse().unwrap()).unwrap();
    gst::Element::link_many(built.elements.iter()).unwrap();
    gst::Element::link_many([built.tee.as_ref().unwrap(), &down, sink.upcast_ref()]).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    for k in 0..count {
        src.push_buffer(picture(33 * k as u64, fields, |x, y| luma(k, x, y))).unwrap();
    }
    src.end_of_stream().unwrap();
    let mut frames = Vec::new();
    while let Some(s) = sink.try_pull_sample(gst::ClockTime::from_seconds(10)) {
        let st = s.caps().unwrap().structure(0).unwrap().to_owned();
        assert_eq!(st.get::<&str>("interlace-mode").unwrap_or("progressive"), "progressive", "caps after the stage: {st}");
        let map = s.buffer().unwrap().map_readable().unwrap();
        frames.push(map[..W * H].to_vec());
        if frames.len() == count {
            break;
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    assert!(!frames.is_empty(), "no picture came out");
    frames
}

fn moving(k: usize, x: usize, y: usize) -> u8 {
    let left = 40 + 8 * k + if y % 2 == 1 { 4 } else { 0 };
    if (left..left + 60).contains(&x) {
        WHITE
    } else {
        BLACK
    }
}

fn comb(l: &[u8]) -> u32 {
    let mut sum = 0u32;
    for y in (20..H - 20).step_by(2) {
        for x in 0..W {
            sum += u32::from(l[y * W + x].abs_diff(l[(y + 1) * W + x]));
        }
    }
    sum
}

#[test]
#[ignore = "needs an NVIDIA card"]
fn cuda_deint_takes_the_comb_off_and_keeps_still_detail() {
    assert!(super::cuda_deint::available().is_ok(), "{:?}", super::cuda_deint::available());
    let woven = run(&node(json!({"deinterlace": "off"})), 8, true, &moving);
    let before = comb(woven.last().unwrap());
    assert!(before > 10_000, "woven, the bar's edges comb: {before}");
    for mode in ["on", "auto"] {
        let out = run(&node(json!({"deinterlace": mode})), 8, true, &moving);
        let after = comb(out.last().unwrap());
        eprintln!("deinterlace {mode}: comb {after} against {before} woven");
        assert!(after * 3 < before, "deinterlace {mode} leaves a comb: {after} against {before} woven");
    }
    // A picture not flagged as fields is passed by `auto` as it came.
    let kept = run(&node(json!({"deinterlace": "auto"})), 8, false, &moving);
    assert_eq!(comb(kept.last().unwrap()), before, "auto deinterlaced a picture not flagged as fields");
    // Still fine lines (one line white, one black) survive once there is a
    // picture before to compare with: nothing moved, so the weave is kept.
    let lines = |_: usize, _: usize, y: usize| if y % 2 == 0 { WHITE } else { BLACK };
    let still = run(&node(json!({"deinterlace": "on"})), 4, true, &lines);
    let last = still.last().unwrap();
    assert!(last[101 * W + 160] < 40 && last[100 * W + 160] > 200, "still lines lost: {} {}", last[100 * W + 160], last[101 * W + 160]);
}

/// Comb over a whole luma plane of any size.
fn comb_of(l: &[u8], w: usize, h: usize) -> u64 {
    let mut sum = 0u64;
    for y in (0..h - 1).step_by(2) {
        for x in 0..w {
            sum += u64::from(l[y * w + x].abs_diff(l[(y + 1) * w + x]));
        }
    }
    sum
}

/// A real NVDEC picture, from an interlaced MPEG-2 file named by
/// GMX_DEINT_TS, through the scale node at the source's size.
fn decoded(mode: &str, path: &str) -> Vec<u64> {
    gmx_netkit::init().unwrap();
    let n = NodeSpec {
        id: "scale:nvdec".into(),
        kind: "scale".into(),
        input: "decode".into(),
        raw: json!({"kind": "scale", "width": 1920, "height": 1080, "fps": [30000, 1001], "fit": "pad", "border": 0xff00_0000u64, "rate": false, "memory": "cuda", "deinterlace": mode}),
    };
    let built = build(&n).unwrap();
    let pipeline = gst::parse::launch(&format!("filesrc location={path} ! tsdemux ! mpegvideoparse ! nvmpeg2videodec name=dec")).unwrap().downcast::<gst::Pipeline>().unwrap();
    super::cuda::share(&pipeline);
    let dec = pipeline.by_name("dec").unwrap();
    let down = gst::ElementFactory::make("cudadownload").build().unwrap();
    let sink = gst::ElementFactory::make("appsink").build().unwrap().downcast::<AppSink>().unwrap();
    sink.set_caps(Some(&"video/x-raw,format=NV12".parse().unwrap()));
    sink.set_sync(false);
    pipeline.add_many(built.elements.iter()).unwrap();
    pipeline.add_many([&down, sink.upcast_ref()]).unwrap();
    dec.link_filtered(built.head.as_ref().unwrap(), &"video/x-raw(memory:CUDAMemory)".parse().unwrap()).unwrap();
    gst::Element::link_many(built.elements.iter()).unwrap();
    gst::Element::link_many([built.tee.as_ref().unwrap(), &down, sink.upcast_ref()]).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let mut out = Vec::new();
    while let Some(s) = sink.try_pull_sample(gst::ClockTime::from_seconds(10)) {
        let vi = gstreamer_video::VideoInfo::from_caps(s.caps().unwrap()).unwrap();
        let buf = s.buffer_owned().unwrap();
        let frame = gstreamer_video::VideoFrame::from_buffer_readable(buf, &vi).unwrap();
        let (w, h, stride) = (vi.width() as usize, vi.height() as usize, frame.plane_stride()[0] as usize);
        let plane = frame.plane_data(0).unwrap();
        let luma: Vec<u8> = (0..h).flat_map(|y| plane[y * stride..y * stride + w].iter().copied()).collect();
        out.push(comb_of(&luma, w, h));
    }
    let _ = pipeline.set_state(gst::State::Null);
    out
}

#[test]
#[ignore = "needs an NVIDIA card and GMX_DEINT_TS"]
fn cuda_deint_on_real_nvdec_pictures() {
    let Ok(path) = std::env::var("GMX_DEINT_TS") else { return };
    let woven = decoded("off", &path);
    let on = decoded("auto", &path);
    // The leaky branch queue drops a few pictures of a file decoded faster
    // than real time, so compare the mean comb of a picture.
    assert!(woven.len() > 10 && on.len() > 10, "pictures: {} woven, {} deinterlaced", woven.len(), on.len());
    let a = woven.iter().sum::<u64>() / woven.len() as u64;
    let b = on.iter().sum::<u64>() / on.len() as u64;
    eprintln!("NVDEC 1080i: mean comb {b} deinterlaced against {a} woven ({} and {} pictures)", on.len(), woven.len());
    assert!(b * 2 < a, "the comb stayed: {b} against {a}");
}
