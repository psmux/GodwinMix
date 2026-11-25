//! A guest's source added well into a show, as every real guest's is, fed
//! the way a guest's session feeds it (across the inter channel) and then
//! left with nothing, as when the guest leaves. It has to stay live the
//! whole time and hold its memory.
//!
//! The inter sources count their frames from zero, so before they were
//! stamped from the clock a source added 25 seconds in was judged stalled
//! within seconds, restarted over and over, and the show's memory grew by a
//! canvas frame thirty times a second until the guard stepped in.

use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::{self, Command, Mixer};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::time::Duration;

fn rss_mb() -> u64 {
    let pid = std::process::id().to_string();
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &pid]).output();
    out.ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u64>().ok()).unwrap_or(0) / 1024
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guest_source_added_late_stays_live_through_the_guest_coming_and_going() {
    gst::init().unwrap();
    if ["intervideosrc", "interaudiosrc"].iter().any(|e| gst::ElementFactory::find(e).is_none()) {
        eprintln!("skipping: the inter elements are not installed");
        return;
    }
    let cfg: Config = toml::from_str("[canvas]\nwidth = 640\nheight = 360\nfps = 30\n\n[control]\nbind = \"127.0.0.1:0\"\n").unwrap();
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    tokio::time::sleep(Duration::from_secs(8)).await;
    let source = SourceConfig::bare("guest-late", &godwinmix_core::guest::source::uri_of("late"));
    handle.request(|ack| Command::AddSource(Box::new(source), Some(ack))).await.unwrap();
    let feed = gst::parse::launch(&format!(
        "videotestsrc is-live=true ! video/x-raw,width=1280,height=720,framerate=30/1 ! videoconvert ! video/x-raw,format=I420 \
         ! intervideosink channel={c} audiotestsrc is-live=true ! audioconvert ! audioresample \
         ! audio/x-raw,format=F32LE,rate=48000,channels=2,layout=interleaved ! interaudiosink channel={c}",
        c = godwinmix_core::guest::channel("late")
    ))
    .unwrap();
    feed.set_state(gst::State::Playing).unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let start = rss_mb();
    for i in 0..8 {
        if i == 4 {
            feed.set_state(gst::State::Null).unwrap();
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
        let status = handle.status().await.unwrap();
        let s = status.sources.iter().find(|s| s.id == "guest-late").expect("the guest's source");
        assert!(
            matches!(s.state, godwinmix_protocol::types::SourceState::Live),
            "{}s after the feed {}: the guest's source is {:?}, idle {:?} ms",
            3 * (i + 1),
            if i < 4 { "started" } else { "stopped" },
            s.state,
            s.video_idle_ms
        );
    }
    let grew = rss_mb().saturating_sub(start);
    handle.send(Command::Shutdown).ok();
    let _ = thread.join();
    assert!(grew < 100, "the show grew by {grew} MB in 24 seconds with one guest");
}
