//! `gmxbussink` into `gmxbussrc`, with real GStreamer elements.

#![cfg(all(unix, feature = "gst"))]

use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;

fn setup() -> String {
    gst::init().unwrap();
    godwinmix_framebus::gst::register().unwrap();
    let dir = format!("/tmp/fbe-{}-{}", std::process::id(), rand_suffix());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rand_suffix() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

fn owner(dir: &str, w: u32, h: u32) -> gst::Pipeline {
    // Solid green: Y is 145 in BT.601 limited range, the same in every pixel.
    let p = gst::parse::launch(&format!(
        "videotestsrc is-live=true pattern=solid-color foreground-color=0xff00ff00 ! \
         video/x-raw,format=NV12,width={w},height={h},framerate=30/1 ! \
         gmxbussink bus-name=camera:el-test bus-dir={dir}"
    ))
    .unwrap();
    let p = p.downcast::<gst::Pipeline>().unwrap();
    p.set_state(gst::State::Playing).unwrap();
    p
}

fn reader(dir: &str) -> (gst::Pipeline, gst_app::AppSink) {
    let p = gst::parse::launch(&format!(
        "gmxbussrc bus-name=camera:el-test bus-dir={dir} ! appsink name=out sync=false max-buffers=2 drop=true"
    ))
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let sink = p
        .by_name("out")
        .unwrap()
        .downcast::<gst_app::AppSink>()
        .unwrap();
    p.set_state(gst::State::Playing).unwrap();
    (p, sink)
}

/// Check one sample's caps and that every luma byte is what the owner drew,
/// reading through the buffer's video meta as any downstream element would.
fn check(sample: &gst::Sample, w: u32, h: u32) -> u8 {
    let caps = sample.caps().unwrap();
    let info = gst_video::VideoInfo::from_caps(caps).unwrap();
    assert_eq!(
        (info.format(), info.width(), info.height()),
        (gst_video::VideoFormat::Nv12, w, h)
    );
    let buffer = sample.buffer().unwrap();
    let meta = buffer
        .meta::<gst_video::VideoMeta>()
        .expect("a video meta with the slot's strides");
    assert_eq!(
        meta.stride()[0] as usize % godwinmix_framebus::format::ROW_ALIGN,
        0
    );
    let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).unwrap();
    let y = frame.plane_data(0).unwrap();
    let stride = frame.plane_stride()[0] as usize;
    let first = y[0];
    for row in 0..h as usize {
        assert!(y[row * stride..row * stride + w as usize]
            .iter()
            .all(|&v| v == first));
    }
    assert!(buffer.meta::<gst::ReferenceTimestampMeta>().is_some());
    first
}

#[test]
fn a_reader_started_first_waits_for_the_owner_and_follows_a_new_size() {
    let dir = setup();
    let (rp, sink) = reader(&dir);
    std::thread::sleep(Duration::from_millis(300));
    let op = owner(&dir, 320, 240);
    let s = sink
        .try_pull_sample(gst::ClockTime::from_seconds(5))
        .expect("a frame from the owner");
    let luma = check(&s, 320, 240);
    assert!(
        (140..=150).contains(&luma),
        "green is about 145 in luma, got {luma}"
    );
    drop(s);
    op.set_state(gst::State::Null).unwrap();
    let op = owner(&dir, 640, 360);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let s = sink
            .try_pull_sample(gst::ClockTime::from_seconds(1))
            .expect("frames keep coming");
        let w = gst_video::VideoInfo::from_caps(s.caps().unwrap())
            .unwrap()
            .width();
        if w == 640 {
            check(&s, 640, 360);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "never saw the new size"
        );
    }
    op.set_state(gst::State::Null).unwrap();
    rp.set_state(gst::State::Null).unwrap();
}
