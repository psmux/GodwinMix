//! The programme encoder started long after the programme.
//!
//! The encoder is attached on demand, so the first output of the day arrives
//! at a programme that may have been running for hours. Its audio chain has an
//! `audiorate` in it, and an `audiorate` that has not been told to skip to its
//! first buffer fills from the start of the segment: the first output added an
//! hour in was handed an hour of silence in one burst, stamped from zero,
//! beside video stamped an hour in. Found on 2026-10-01 through the scale
//! harness, where a udp output's remuxer waited on that audio for its video
//! and never sent again. The audio and the video an output first sees have to
//! start at the same moment.

use godwinmix_core::mixer::{self, Command, Mixer};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn config() -> godwinmix_core::config::Config {
    let mut cfg: godwinmix_core::config::Config = toml::from_str("").unwrap();
    cfg.canvas = godwinmix_core::config::Canvas { width: 320, height: 180, fps: 15, sample_rate: 48000, channels: 2 };
    cfg.multiview.enabled = false;
    cfg.program.encoder = "on-demand".into();
    cfg
}

/// The running time of the first buffer through `tee`'s sink pad. Running
/// time rather than PTS, because an encoder may offset its timestamps (x264
/// starts at a thousand hours) and say so in its segment.
fn first_pts(pipeline: &gst::Pipeline, tee: &str) -> Arc<Mutex<Option<gst::ClockTime>>> {
    let seen = Arc::new(Mutex::new(None));
    let slot = seen.clone();
    let pad = pipeline.by_name(tee).and_then(|t| t.static_pad("sink")).expect("the encoded tee");
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let Some(gst::PadProbeData::Buffer(b)) = &info.data else { return gst::PadProbeReturn::Ok };
        let segment = pad.sticky_event::<gst::event::Segment>(0);
        let running = segment.and_then(|e| {
            e.segment().downcast_ref::<gst::ClockTime>().and_then(|s| s.to_running_time(b.pts()?))
        });
        if let Some(t) = running {
            slot.lock().unwrap().get_or_insert(t);
        }
        gst::PadProbeReturn::Ok
    });
    seen
}

#[tokio::test(flavor = "multi_thread")]
async fn an_encoder_started_late_starts_its_audio_where_its_video_starts() {
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, _bus_rx) = Mixer::build(config()).unwrap();
    mix.start().unwrap();
    let pipeline = mix.program_pipeline().clone();
    let enc = mix.encoder_handle();
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());

    // The programme runs for a while with nobody reading it.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let video = first_pts(&pipeline, "venc-tee");
    let audio = first_pts(&pipeline, "aenc-tee");
    let lease = enc.lease("test");
    for _ in 0..100 {
        if video.lock().unwrap().is_some() && audio.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (v, a) = (*video.lock().unwrap(), *audio.lock().unwrap());
    drop(lease);
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();

    let (v, a) = (v.expect("no encoded video"), a.expect("no encoded audio"));
    let apart = v.mseconds().abs_diff(a.mseconds());
    assert!(v.mseconds() > 2000, "the video started at {v}, before the encoder was asked for");
    assert!(apart < 500, "the first encoded audio is at {a} and the first video at {v}: {apart} ms apart");
}
