//! Sound through `gmxbussink` and `gmxbussrc`: every chunk, in order, with
//! the owner's timestamps when asked for them.

#![cfg(all(unix, feature = "gst"))]

use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

fn setup() -> String {
    gst::init().unwrap();
    godwinmix_framebus::gst::register().unwrap();
    let dir = format!("/tmp/fbs-{}", std::process::id());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn launch(text: &str) -> gst::Pipeline {
    let p = gst::parse::launch(text).unwrap().downcast::<gst::Pipeline>().unwrap();
    p.set_state(gst::State::Playing).unwrap();
    p
}

#[test]
fn sound_arrives_whole_and_in_order_with_the_owners_timestamps() {
    let dir = setup();
    // A reader that lost a chunk would show a hole in the pts. 100 ms chunks,
    // the most a slot holds, not 10 ms: a reader more than eight chunks
    // behind skips ahead by design (`ring::reader::CATCH_UP`), and with 10 ms
    // chunks a reader thread held back 80 ms on a busy macOS runner skipped
    // one. That is the ring doing its job, not losing sound.
    let owner = launch(&format!(
        "audiotestsrc is-live=true samplesperbuffer=4800 ! \
         audio/x-raw,format=F32LE,layout=interleaved,rate=48000,channels=2 ! \
         gmxbussink bus-name=camera:snd#audio bus-dir={dir}"
    ));
    let reader = launch(&format!(
        "gmxbussrc bus-name=camera:snd#audio bus-dir={dir} timestamps=owner ! \
         appsink name=out sync=false"
    ));
    let sink = reader.by_name("out").unwrap().downcast::<gst_app::AppSink>().unwrap();
    let (mut got_ns, mut holes, mut last_end) = (0u64, 0u32, None::<u64>);
    let mut caps_ok = false;
    let end = Instant::now() + Duration::from_secs(2);
    while Instant::now() < end {
        let Some(s) = sink.try_pull_sample(gst::ClockTime::from_mseconds(200)) else { continue };
        let caps = s.caps().unwrap().structure(0).unwrap().to_owned();
        caps_ok = caps.name() == "audio/x-raw"
            && caps.get::<i32>("rate").unwrap() == 48000
            && caps.get::<i32>("channels").unwrap() == 2;
        let b = s.buffer().unwrap();
        let (pts, dur) = (b.pts().unwrap().nseconds(), b.duration().unwrap().nseconds());
        assert_eq!(b.size() as u64, dur * 48 * 8 / 1_000_000, "bytes and duration agree");
        if let Some(prev) = last_end {
            if pts.abs_diff(prev) > 1_000_000 {
                holes += 1;
            }
        }
        last_end = Some(pts + dur);
        got_ns += dur;
    }
    owner.set_state(gst::State::Null).unwrap();
    reader.set_state(gst::State::Null).unwrap();
    assert!(caps_ok, "the reader's caps are the owner's");
    assert_eq!(holes, 0, "every chunk arrived, one after another");
    assert!(got_ns > 1_500_000_000, "{} ms of sound in 2 s", got_ns / 1_000_000);
}
