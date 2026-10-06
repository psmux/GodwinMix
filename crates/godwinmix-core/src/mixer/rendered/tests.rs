//! Renditions on a real programme: shared encoders, a rung added and
//! removed with nothing else touched, keyframes on the same frames, the
//! governor's refusal and its shedding. Real GStreamer, x264 so it runs on
//! any machine.

use super::super::Mixer;
use crate::config::{Accel, Config, OutputConfig};
use godwinmix_protocol::rendition::{
    AudioWant, Container, Fps, RenditionChoice, RenditionRequest, VideoCodec, VideoWant,
};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub(super) fn config() -> Config {
    let mut cfg: Config = toml::from_str("").unwrap();
    cfg.canvas = crate::config::Canvas { width: 320, height: 180, fps: 30, sample_rate: 48000, channels: 2 };
    cfg.multiview.enabled = false;
    // x264, so the test means the same on a machine with no GPU.
    cfg.hardware.encode = Accel::Software;
    cfg
}

pub(super) async fn mixer(cfg: Config) -> Mixer {
    let _ = gst::init();
    let (mut mix, _h, _c, _b) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    mix
}

pub(super) fn want(height: u32, kbps: u32) -> RenditionChoice {
    RenditionChoice::Request(RenditionRequest {
        id: "x".into(),
        container: Container::Flv,
        video: Some(VideoWant {
            codec: Some(VideoCodec::H264),
            height: Some(height),
            fps: Some(Fps::whole(30)),
            bitrate_kbps: Some(kbps),
            keyframe_ms: Some(1000),
            ..VideoWant::default()
        }),
        audio: Some(AudioWant { bitrate_kbps: Some(96), ..AudioWant::default() }),
        ..RenditionRequest::default()
    })
}

pub(super) fn output(id: &str, rendition: Option<RenditionChoice>) -> OutputConfig {
    OutputConfig { rendition, ..OutputConfig::bare(id, &format!("rtmp://127.0.0.1:1/live/{id}")) }
}

/// Every element of the programme pipeline whose name says it is a
/// rendition encoder.
pub(super) fn encoders(mix: &Mixer) -> Vec<String> {
    let mut v: Vec<String> = mix
        .program
        .iterate_recurse()
        .into_iter()
        .flatten()
        .map(|e| e.name().to_string())
        .filter(|n| n.starts_with("r-encode") && n.ends_with("-enc"))
        .collect();
    v.sort();
    v
}

/// The largest gap between two buffers on a pad, by timestamp.
#[derive(Default)]
pub(super) struct Gaps {
    last: AtomicU64,
    largest: AtomicU64,
    seen: AtomicU64,
}

impl Gaps {
    pub(super) fn watch(self: &Arc<Self>, pad: &gst::Pad) {
        let me = self.clone();
        pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
            if let Some(pts) = info.buffer().and_then(|b| b.pts()) {
                let last = me.last.swap(pts.nseconds(), Ordering::Relaxed);
                if last > 0 && pts.nseconds() > last {
                    me.largest.fetch_max(pts.nseconds() - last, Ordering::Relaxed);
                }
                me.seen.fetch_add(1, Ordering::Relaxed);
            }
            gst::PadProbeReturn::Ok
        });
    }

    pub(super) async fn wait_for(&self, frames: u64) {
        let mark = self.seen.load(Ordering::Relaxed) + frames;
        for _ in 0..500 {
            if self.seen.load(Ordering::Relaxed) >= mark {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no buffers arrived");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn four_outputs_that_want_one_rendition_share_one_encoder() {
    let mut mix = mixer(config()).await;
    for id in ["a", "b", "c", "d"] {
        mix.add_output(&output(id, Some(want(144, 400)))).expect("each output is admitted");
    }
    assert_eq!(encoders(&mix).len(), 1, "four outputs, one video encoder: {:?}", encoders(&mix));
    let plan = mix.renditions().shared_view().read().clone();
    let encode = plan.nodes.iter().find(|n| n.kind == "encode").expect("an encode node");
    assert_eq!(encode.serves.len(), 4);
    assert!(!mix.encoder_handle().is_running(), "nothing reads the programme encoder, so it is off");
    for id in ["a", "b", "c", "d"] {
        mix.remove_output(&id.to_string()).unwrap();
    }
    assert!(encoders(&mix).is_empty(), "the last output took its encoder with it");
    mix.shutdown();
}

/// The acceptance line: adding and removing a rung touches only its own
/// nodes. The programme encoder (read by a plain output) and a rendition
/// another output reads both run straight through it without a gap.
#[tokio::test(flavor = "multi_thread")]
async fn a_rung_comes_and_goes_and_nothing_else_drops_a_buffer() {
    let mut mix = mixer(config()).await;
    mix.add_output(&output("plain", None)).unwrap();
    mix.add_output(&output("steady", Some(want(144, 400)))).unwrap();
    let programme = Arc::new(Gaps::default());
    programme.watch(&mix.venc_tee.static_pad("sink").unwrap());
    let steady_tee = mix.rendition_taps("steady")[0].video_tee.clone().unwrap();
    let steady = Arc::new(Gaps::default());
    steady.watch(&steady_tee.static_pad("sink").unwrap());
    programme.wait_for(15).await;
    steady.wait_for(15).await;
    let before = encoders(&mix);
    programme.largest.store(0, Ordering::Relaxed);
    steady.largest.store(0, Ordering::Relaxed);

    mix.add_output(&output("rung", Some(want(90, 200)))).unwrap();
    assert_eq!(encoders(&mix).len(), before.len() + 1, "one encoder started for the rung");
    steady.wait_for(20).await;
    mix.remove_output(&"rung".to_string()).unwrap();
    assert_eq!(encoders(&mix), before, "only the rung's encoder stopped");
    steady.wait_for(20).await;
    programme.wait_for(5).await;

    let frame = 1_000_000_000 / 30;
    for (name, g) in [("programme", &programme), ("the other rendition", &steady)] {
        let largest = g.largest.load(Ordering::Relaxed);
        println!("{name}: largest interval {:.1} ms while a rung came and went", largest as f64 / 1e6);
        assert!(largest < frame * 3 / 2, "{name} lost a frame: largest interval {largest} ns");
    }
    mix.shutdown();
}

/// Keyframes at the same running times on two rungs started at different
/// moments, which is what lets a player switch between them.
#[tokio::test(flavor = "multi_thread")]
async fn two_rungs_started_apart_put_keyframes_on_the_same_frames() {
    let mut mix = mixer(config()).await;
    let keys = |tee: gst::Element| {
        let seen = Arc::new(Mutex::new(Vec::<u64>::new()));
        let s = seen.clone();
        tee.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
            if let Some(b) = info.buffer().filter(|b| !b.flags().contains(gst::BufferFlags::DELTA_UNIT)) {
                let at = b.pts().and_then(|p| crate::render::keyframes::running_time(pad, p));
                // To the millisecond: two rungs' frames carry one time, but
                // each encoder's own segment may round it differently.
                s.lock().extend(at.map(|t| t.mseconds()));
            }
            gst::PadProbeReturn::Ok
        });
        seen
    };
    // The programme running first, so the two rungs start on different
    // frames of it rather than both on its first.
    let running = Arc::new(Gaps::default());
    running.watch(&mix.vraw_tee.static_pad("sink").unwrap());
    running.wait_for(10).await;
    mix.add_output(&output("big", Some(want(144, 400)))).unwrap();
    let big = keys(mix.rendition_taps("big")[0].video_tee.clone().unwrap());
    tokio::time::sleep(Duration::from_millis(700)).await;
    mix.add_output(&output("small", Some(want(90, 200)))).unwrap();
    let small = keys(mix.rendition_taps("small")[0].video_tee.clone().unwrap());
    tokio::time::sleep(Duration::from_millis(3300)).await;
    let (big, small) = (big.lock().clone(), small.lock().clone());
    println!("keyframes big {big:?} small {small:?}");
    // The small rung's first keyframe is its start; every one after it is
    // on an interval boundary the big rung also has.
    // Within a millisecond, since the truncation to milliseconds can land
    // either side of one: macOS gave big 2021 and small 2020 for one frame,
    // which is a thirtieth of the 33 ms between two frames.
    let near = |a: u64, b: u64| a.abs_diff(b) <= 1;
    let shared: Vec<&u64> = small.iter().skip(1).filter(|p| big.iter().any(|b| near(*b, **p))).collect();
    assert!(small.len() >= 3, "the small rung made keyframes: {small:?}");
    assert_ne!(small[0], big[0], "the rungs started on different frames");
    let gaps: Vec<u64> = big.windows(2).skip(1).map(|w| w[1] - w[0]).collect();
    assert!(gaps.iter().all(|g| near(*g, 1000)), "one keyframe a second, as asked: {big:?}");
    assert_eq!(shared.len(), small.len() - 1, "every keyframe after the first lines up: big {big:?} small {small:?}");
    mix.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn what_does_not_fit_is_refused_with_advice_and_leaves_nothing() {
    let mut cfg = config();
    // Every core but a tenth kept for something else on the machine.
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f32;
    cfg.governor.reserve_cores = Some(cores - 0.1);
    let mut mix = mixer(cfg).await;
    let big = RenditionChoice::Request(RenditionRequest {
        id: "x".into(),
        video: Some(VideoWant { codec: Some(VideoCodec::H264), width: Some(1920), height: Some(1080), fps: Some(Fps::whole(60)), ..VideoWant::default() }),
        ..RenditionRequest::default()
    });
    let e = mix.add_output(&output("huge", Some(big))).expect_err("refused");
    let refusal = e.chain().find_map(|c| c.downcast_ref::<crate::render::Refusal>()).expect("a refusal with data");
    assert_eq!(refusal.code, godwinmix_protocol::error::ErrorCode::Safety);
    assert!(refusal.data.get("need").is_some() && refusal.data.get("advice").is_some(), "{}", refusal.data);
    println!("refused: {}", refusal.message);
    assert!(encoders(&mix).is_empty());
    assert!(mix.status().outputs.is_empty(), "the refused output is not there");
    assert!(mix.renditions().shared_view().read().nodes.is_empty());
    mix.shutdown();
}

mod hls;
mod shed;
mod unattached;
