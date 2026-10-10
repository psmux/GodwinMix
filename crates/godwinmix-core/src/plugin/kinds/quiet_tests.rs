//! A network source that goes quiet behind `livesync` reads stalled.
//!
//! The shape of the cable pull on 2026-10-10, with no network: an appsrc
//! stands in for the RTMP client, sends a second of frames and then nothing,
//! without an end of stream, the way a pulled cable looks from this side.
//! livesync goes on repeating the last frame into the proxy, which is what
//! kept the source reading `live` for two minutes.

use super::*;
use crate::config::{Accel, BrowserConfig, Canvas, SourceConfig};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

fn ctx() -> BuildCtx {
    let canvas = CanvasCaps::new(&Canvas {
        width: 160,
        height: 90,
        fps: 30,
        sample_rate: 48_000,
        channels: 2,
    });
    BuildCtx {
        id: "quiet".into(),
        cfg: SourceConfig::bare("quiet", "rtmp://127.0.0.1/live/quiet"),
        canvas,
        backends: Backends::probe(Accel::Software, Accel::Software).expect("software backends"),
        thumb_fps: 1,
        browser: BrowserConfig::default(),
        allow_exec: false,
        origin: Instant::now(),
        tier: Tier::Core,
    }
}

#[test]
fn a_feed_that_goes_quiet_behind_livesync_is_stalled() {
    let _ = gst::init();
    if !crate::probe::exists("livesync") {
        eprintln!("skipped: livesync is not installed, so there is nothing repeating frames");
        return;
    }
    let ctx = ctx();
    let src = gstreamer_app::AppSrc::builder()
        .name("quiet-feed")
        .format(gst::Format::Time)
        .is_live(true)
        .do_timestamp(true)
        .caps(&CanvasCaps::video_at(160, 90, gst::Fraction::new(30, 1)))
        .build();
    let ends = assemble(
        &ctx,
        false,
        Ingest::default().with([src.clone().upcast::<gst::Element>()]).livesync(true),
        |w: &Wiring| {
            src.link(&w.norm.video_entry()).context("linking the feed")?;
            Ok(KindParts::default())
        },
    )
    .expect("the pipeline assembles");
    // Count what reaches the proxy, to show livesync really is repeating.
    let below = Arc::new(AtomicU64::new(0));
    let counted = below.clone();
    ends.video.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    ends.pipeline.set_state(gst::State::Playing).expect("playing");
    for _ in 0..30 {
        let buffer = gst::Buffer::from_mut_slice(vec![0u8; 160 * 90 * 4]);
        let _ = src.push_buffer(buffer);
        std::thread::sleep(Duration::from_millis(33));
    }
    assert!(ends.health.saw_video(), "nothing reached the proxy while the feed was sending");
    let before = below.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(1500));
    let repeated = below.load(Ordering::Relaxed) - before;
    let stalled = ends.health.is_stalled(1.0);
    let idle = ends.health.video_idle_ms();
    let _ = ends.pipeline.set_state(gst::State::Null);
    assert!(stalled, "a quiet feed read live: idle {idle:?} ms, {repeated} repeats below livesync");
}
