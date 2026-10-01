//! What the vitals cost, measured: `cargo test -p gmx-ingest --release
//! direct::vitals::bench -- --ignored --nocapture`.
//!
//! Ten seconds of 1080p30 H.264 (a moving zone plate, fine detail everywhere,
//! a keyframe a second, 6 Mbit/s) and AAC are
//! encoded once, then replayed in real time into N hub publications from one
//! thread, the way N direct shows' inputs would arrive. The process's own CPU
//! time and resident memory are read with `ps` over each phase, and the
//! replay alone is measured first so it can be taken off.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;

use super::Vitals;
use ways::phase;
use crate::hub::{Hub, Publication};
use crate::media_tag::MediaTag;
use crate::rtmp::Inlet;
use crate::tagger;

struct Collect(Arc<Mutex<Vec<MediaTag>>>);

impl Inlet for Collect {
    fn tag(&mut self, tag: MediaTag) {
        self.0.lock().unwrap().push(tag);
    }
}

/// Ten seconds of 1080p, encoded as fast as the encoder goes.
pub fn clip() -> Vec<MediaTag> {
    gmx_netkit::init().unwrap();
    let tags: Arc<Mutex<Vec<MediaTag>>> = Arc::default();
    let to = tagger::share(Box::new(Collect(tags.clone())));
    let zero = Arc::new(tagger::Zero::default());
    let line = "videotestsrc num-buffers=300 pattern=zone-plate kx2=20 ky2=20 kt=1 ! video/x-raw,width=1920,height=1080,framerate=30/1 \
        ! x264enc speed-preset=veryfast key-int-max=30 bitrate=6000 ! h264parse name=vp \
        audiotestsrc num-buffers=470 ! audio/x-raw,rate=48000,channels=2 ! avenc_aac ! aacparse name=ap";
    let p = gst::parse::launch(line).unwrap().downcast::<gst::Pipeline>().unwrap();
    for (parser, sink) in [("vp", tagger::video_sink(to.clone(), zero.clone())), ("ap", tagger::audio_sink(to, zero))] {
        p.add(&sink).unwrap();
        p.by_name(parser).unwrap().link(&sink).unwrap();
    }
    p.set_state(gst::State::Playing).unwrap();
    let bus = p.bus().unwrap();
    bus.timed_pop_filtered(gst::ClockTime::from_seconds(120), &[gst::MessageType::Eos, gst::MessageType::Error]);
    p.set_state(gst::State::Null).unwrap();
    let mut t = tags.lock().unwrap().clone();
    t.sort_by_key(|t| (t.timestamp_ms, !t.sequence_header));
    t
}

/// Replay `clip` into `n` publications in real time, looping, until `stop`.
fn replay(hub: &Hub, clip: Vec<MediaTag>, n: usize, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    let pubs: Vec<Publication> = (0..n).map(|i| hub.publish(&format!("show-{i}"), "main", "bench", None).unwrap()).collect();
    std::thread::spawn(move || {
        let (start, span) = (Instant::now(), clip.last().map(|t| t.timestamp_ms + 33).unwrap_or(1000));
        let headers: Vec<&MediaTag> = clip.iter().filter(|t| t.sequence_header).collect();
        for p in &pubs {
            headers.iter().for_each(|h| { p.push((*h).clone()); });
        }
        let mut lap = 0u32;
        while !stop.load(Ordering::Relaxed) {
            for tag in clip.iter().filter(|t| !t.sequence_header) {
                let at = Duration::from_millis(u64::from(lap * span + tag.timestamp_ms));
                if let Some(wait) = at.checked_sub(start.elapsed()) {
                    std::thread::sleep(wait);
                }
                let mut t = tag.clone();
                t.timestamp_ms += lap * span;
                pubs.iter().for_each(|p| { p.push(t.clone()); });
                if stop.load(Ordering::Relaxed) {
                    return;
                }
            }
            lap += 1;
        }
    })
}

/// `VITALS_CHECKS=picture` or `sound` measures one half alone.
fn checks() -> serde_json::Value {
    match std::env::var("VITALS_CHECKS").as_deref() {
        Ok("picture") => json!({"silence_secs": 0}),
        Ok("sound") => json!({"black_secs": 0, "freeze_secs": 0}),
        _ => json!({}),
    }
}

fn run(n: usize, wall: usize, workers: usize) {
    let clip = clip();
    let hub = Hub::new();
    let stop = Arc::new(AtomicBool::new(false));
    let feeder = replay(&hub, clip, n, stop.clone());
    let (base, base_mb) = phase(&format!("{n} shows, replay only"), 10);
    let vitals = Vitals::start(hub.clone(), Arc::new(|_, _| {}), workers);
    for i in 0..n {
        let id = format!("show-{i}");
        vitals.watch(&id, &id, "main", &json!({"alarms": true, "pictures": false, "thresholds": checks()}));
    }
    std::thread::sleep(Duration::from_secs(3));
    let (alarms, mb) = phase(&format!("{n} shows, alarms on, nobody looking, {workers} workers"), 20);
    eprintln!("BENCH   vitals alone: {:.1}% of one core, +{:.0} MB; dropped, done: {:?}", alarms - base, mb - base_mb, vitals.counts());
    if wall > 0 {
        let looking = Arc::new(AtomicBool::new(true));
        let (v, l) = (vitals.clone(), looking.clone());
        let asker = std::thread::spawn(move || while l.load(Ordering::Relaxed) {
            (0..wall).for_each(|i| { let _ = v.call("direct.thumbnail", &json!({"show": format!("show-{i}")})); });
            std::thread::sleep(Duration::from_secs(1));
        });
        std::thread::sleep(Duration::from_secs(3));
        let (walled, mb) = phase(&format!("{n} shows, alarms on, a wall of {wall} asking every second"), 20);
        eprintln!("BENCH   vitals alone: {:.1}% of one core, +{:.0} MB; dropped, done: {:?}", walled - base, mb - base_mb, vitals.counts());
        looking.store(false, Ordering::Relaxed);
        asker.join().unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    feeder.join().unwrap();
}

#[test]
#[ignore]
fn one_show_at_1080p() {
    run(1, 1, 1);
}

#[test]
#[ignore]
fn two_hundred_shows_and_a_wall_of_forty() {
    let workers = std::env::var("VITALS_WORKERS").ok().and_then(|w| w.parse().ok()).unwrap_or(3);
    run(200, 40, workers);
}

#[path = "bench_ways.rs"]
mod ways;
