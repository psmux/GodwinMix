//! An event played back: the right frames, slow motion's timestamps, a change
//! of angle while it plays, and the programme on time throughout.

use super::{rig, Rig, SEC};
use godwinmix_core::input::InputPipeline;
use godwinmix_core::mixer::{Command, SeekOutcome};
use godwinmix_core::observe::metrics::counter;
use godwinmix_core::config::SourceConfig;
use godwinmix_core::replay::ReplayCall;
use godwinmix_protocol::replay::{MarkRequest, ReplayConfig, ReplayEvent};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What went into the replay's decoder: time, decode only, and the address of
/// the frame's bytes, which are the ring's own.
type Seen = Arc<Mutex<Vec<(u64, bool, usize)>>>;

fn watch(pad: &gst::Pad) -> Seen {
    let seen: Seen = Arc::default();
    let s = seen.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        if let Some(gst::PadProbeData::Buffer(b)) = &info.data {
            let ptr = b.map_readable().map(|m| m.as_slice().as_ptr() as usize).unwrap_or(0);
            s.lock().push((b.pts().map(|t| t.nseconds()).unwrap_or(0), b.flags().contains(gst::BufferFlags::DECODE_ONLY), ptr));
        }
        gst::PadProbeReturn::Ok
    });
    seen
}

async fn input(rig: &Rig, id: &str) -> Arc<InputPipeline> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rig.handle.send(Command::SourceInput { source: id.into(), reply: tx }).unwrap();
    rx.await.unwrap().expect("the replay source is there")
}

/// Where each frame of `angle`'s ring sits in clock time, by its bytes.
fn ring_frames(rig: &Rig, angle: &str) -> Vec<(u64, usize)> {
    let ring = rig.handle.replay_shared().ring(angle).unwrap();
    let clip = ring.lock().everything();
    clip.video.iter().map(|f| (f.frame.at, f.frame.buffer.map_readable().unwrap().as_slice().as_ptr() as usize)).collect()
}

async fn seek(rig: &Rig, ms: u64) {
    assert!(matches!(rig.handle.seek("replay".into(), ms).await.unwrap(), SeekOutcome::Moved(_)));
}

fn shown(seen: &Seen) -> Vec<(u64, usize)> {
    seen.lock().iter().filter(|(_, hidden, _)| !hidden).map(|(t, _, p)| (*t, *p)).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_event_plays_its_own_frames_at_full_and_half_speed_and_changes_angle() {
    let rig = rig().await;
    let cfg = ReplayConfig { seconds: 20, angles: vec!["cam1".into(), "cam2".into()], bitrate_kbps: 0 };
    rig.call(ReplayCall::Start(cfg)).await.unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    let (_, event) = rig.call(ReplayCall::Mark(MarkRequest { seconds: Some(2.0), angle: Some("cam1".into()), ..Default::default() })).await.unwrap();
    let event: ReplayEvent = event.unwrap();
    let (from, to) = (event.in_ms * 1_000_000, event.out_ms.unwrap() * 1_000_000);

    let mut src = SourceConfig::bare("replay", "replay://");
    src.params.insert("event".into(), event.id.clone().into());
    rig.handle.request(|ack| Command::AddSource(Box::new(src), Some(ack))).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let replay = input(&rig, "replay").await;
    let (pipeline, _, _) = replay.taps();
    let appsrc = pipeline.by_name("replay-src-replay").unwrap();
    let seen = watch(&appsrc.static_pad("src").unwrap());
    let out = pipeline.by_name("replay-vproxy").unwrap();
    let proxied = watch(&out.static_pad("sink").unwrap());
    let (frames0, t0) = (counter("gmx_programme_frames_total", &[]).get(), Instant::now());

    // Full speed: the frames between the marks, and only those, are shown.
    seek(&rig, 0).await;
    tokio::time::sleep(Duration::from_millis(3000)).await;
    let cam1 = ring_frames(&rig, "cam1");
    let want: Vec<usize> = cam1.iter().filter(|(at, _)| *at >= from && *at < to).map(|(_, p)| *p).collect();
    let got: Vec<usize> = shown(&seen).iter().map(|(_, p)| *p).collect();
    assert!(want.len() >= 55, "two seconds of cam1 at 30 fps: {}", want.len());
    assert_eq!(got.first(), want.first(), "the first frame shown is the one at the in point");
    assert_eq!(&got[..want.len().min(got.len())], &want[..want.len().min(got.len())], "and every one after it is the ring's own");
    assert!(seen.lock().iter().any(|(_, hidden, _)| *hidden) || cam1.iter().any(|(at, _)| *at == from), "frames before the in point are decoded, not shown");

    // Half speed: each frame is stamped twice as far apart, and the source
    // still hands the programme 30 frames a second.
    let state = replay.call("load", json!({ "event": event.id, "speed": 0.5 })).unwrap();
    assert!(state["state"]["duration_ms"].as_u64().unwrap() >= 3900, "{state}");
    seen.lock().clear();
    proxied.lock().clear();
    seek(&rig, 0).await;
    tokio::time::sleep(Duration::from_millis(3000)).await;
    let slow = shown(&seen);
    let mean = |ts: &[u64]| {
        let gaps: Vec<u64> = ts.windows(2).filter(|w| w[1] > w[0]).map(|w| w[1] - w[0]).collect();
        gaps.iter().sum::<u64>() / gaps.len().max(1) as u64
    };
    let stamped = mean(&slow.iter().map(|(t, _)| *t).collect::<Vec<_>>());
    assert!((60_000_000..73_000_000).contains(&stamped), "half speed stamps frames 66 ms apart, not {stamped}");
    let out: Vec<u64> = shown(&proxied).iter().map(|(t, _)| *t).collect();
    let cadence = mean(&out);
    assert!(out.len() >= 60 && (32_000_000..35_000_000).contains(&cadence), "the programme gets a frame every 33 ms from it: {cadence} over {}", out.len());

    // A change of angle while it plays lands on cam2's frames, from where it was.
    seek(&rig, 0).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    let moved = replay.call("set", json!({ "angle": "cam2", "speed": 1.0 })).unwrap();
    seen.lock().clear();
    seek(&rig, moved["position_ms"].as_u64().unwrap()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let cam2: Vec<usize> = ring_frames(&rig, "cam2").into_iter().map(|(_, p)| p).collect();
    let after = shown(&seen);
    let landed = moved["position_ms"].as_u64().unwrap();
    assert!(landed > 0 && landed < 2000, "half way through at half speed is part way through at full: {moved}");
    assert!(!after.is_empty() && after.iter().all(|(_, p)| cam2.contains(p)), "every frame after the change is cam2's");

    let frames = counter("gmx_programme_frames_total", &[]).get() - frames0;
    let expected = (t0.elapsed().as_secs_f64() * 30.0) as u64;
    assert!(frames + 15 >= expected, "the programme kept its 30 fps through it all: {frames} of {expected}");
    let _ = SEC;
    rig.stop().await;
}
