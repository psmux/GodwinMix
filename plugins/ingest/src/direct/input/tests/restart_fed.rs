//! A sender restarted with the same layout, with no socket in between: the
//! sender's TS handed to an appsrc in place of a udpsrc, so it runs where
//! loopback UDP does not (a VPN driver on one laptop drops it).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use super::super::loss::Loss;
use super::super::pads::{parse_into, Pads};
use super::super::runner::{run, Plan};
use super::super::super::StopSignal;
use super::Collect;
use crate::media_tag::TagKind;

type Feed = Arc<Mutex<Option<gst_app::AppSrc>>>;

/// What udpsrc does, played by an appsrc: live, each buffer stamped with
/// the time it arrived.
struct Fed(Feed);

impl Plan for Fed {
    fn build(&mut self, pipeline: &gst::Pipeline, pads: &Arc<Pads>) -> Result<Loss, String> {
        let caps = gst::Caps::builder("video/mpegts").field("systemstream", true).field("packetsize", 188).build();
        let src = gst_app::AppSrc::builder().name("src").is_live(true).do_timestamp(true).format(gst::Format::Time).caps(&caps).build();
        pipeline.add(&src).map_err(|e| e.to_string())?;
        *self.0.lock().unwrap() = Some(src.clone());
        let loss = Loss::probe(src.upcast_ref(), None)?;
        parse_into(pipeline, &src.static_pad("src").ok_or("no src pad")?, pads)?;
        Ok(loss)
    }
    fn address(&self) -> String {
        "a fed transport".into()
    }
    fn stall_ms(&self) -> Option<u64> {
        None
    }
}

/// The station test's sender, into `feed` rather than a socket.
fn sender(feed: &Feed, vpid: u16, apid: u16, program: u16) -> gst::Pipeline {
    let text = format!(
        "mpegtsmux name=mux alignment=7 prog-map=program_map,sink_{vpid}={program},sink_{apid}={program} ! appsink name=out sync=false \
         videotestsrc is-live=true horizontal-speed=4 ! video/x-raw,format=I420,width=320,height=180,framerate=30/1 ! x264enc tune=zerolatency key-int-max=30 bitrate=600 ! h264parse ! mux.sink_{vpid} \
         audiotestsrc is-live=true ! audioconvert ! audioresample ! audio/x-raw,rate=48000 ! avenc_aac ! aacparse ! mux.sink_{apid}"
    );
    let p = gst::parse::launch(&text).unwrap().downcast::<gst::Pipeline>().unwrap();
    let out = p.by_name("out").unwrap().downcast::<gst_app::AppSink>().unwrap();
    let feed = feed.clone();
    out.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |s| {
                let sample = s.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let bytes = sample.buffer().and_then(|b| b.map_readable().ok().map(|m| m.to_vec())).unwrap_or_default();
                if let Some(src) = feed.lock().unwrap().as_ref() {
                    let _ = src.push_buffer(gst::Buffer::from_mut_slice(bytes));
                    let level = src.current_level_bytes();
                    if level > 50_000 {
                        eprintln!("backlog {level} bytes at {:?}", std::time::SystemTime::now());
                    }
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    p.set_state(gst::State::Playing).unwrap();
    p
}

/// Video frames a second over `secs`, and the rate the input measured.
fn rate(got: &Collect, secs: u64) -> (f64, f64) {
    let before = got.frames(TagKind::Video);
    std::thread::sleep(Duration::from_secs(secs));
    ((got.frames(TagKind::Video) - before) as f64 / secs as f64, got.last().fps)
}

#[test]
fn a_sender_restarted_with_the_same_layout_keeps_its_whole_frame_rate() {
    let _one = super::one_at_a_time();
    gmx_netkit::init().unwrap();
    if gst::ElementFactory::find("x264enc").is_none() || gst::ElementFactory::find("avenc_aac").is_none() {
        eprintln!("skipping: needs x264enc and avenc_aac");
        return;
    }
    let feed: Feed = Arc::default();
    let (got, stop) = (Collect::default(), StopSignal::default());
    let (sink, s, plan) = (Box::new(got.clone()), stop.clone(), Fed(feed.clone()));
    let thread = std::thread::spawn(move || run(plan, sink, s));
    let mut tx = sender(&feed, 65, 66, 1);
    let started = Instant::now();
    while got.keyframes() < 3 && started.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_secs(12));
    let mut seen = vec![("first", rate(&got, 3))];
    for what in ["new PIDs", "the same layout again", "and again", "a third time", "a fourth"] {
        tx.set_state(gst::State::Null).unwrap();
        std::thread::sleep(Duration::from_millis(2200));
        tx = sender(&feed, 300, 301, 7);
        // The first two seconds hold the new sender's own start.
        for k in 0..3 {
            let r = rate(&got, 2);
            if k > 0 {
                seen.push((what, r));
            }
        }
    }
    tx.set_state(gst::State::Null).unwrap();
    stop.stop();
    thread.join().unwrap();
    eprintln!("{seen:?}");
    for (what, (arrived, measured)) in &seen {
        assert!(*arrived > 24.0, "{what}: {arrived} frames a second arrived: {seen:?}");
        assert!(*measured > 24.0, "{what}: the input measured {measured} fps: {seen:?}");
    }
}
