//! Readers: on the frame bus, on `unixfdsrc`, or decoding the clip itself.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use godwinmix_framebus::{monotonic_ns, BusName, Registry, Subscriber};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::measure::{cpu_over, quantile, report, sleep_until};
use crate::{clip, Args};

#[derive(Default)]
struct Tally {
    frames: u64,
    skipped: u64,
    latency_us: Vec<u64>,
}

pub fn run(args: &Args) {
    match args.get("mechanism", "bus").as_str() {
        "unixfd" => {
            let stall = args.num("stall-ms", 0);
            let hold = if stall > 0 { format!("identity sleep-time={} ! ", stall * 1000) } else { String::new() };
            let pipeline = format!("unixfdsrc socket-path={} ! {hold}appsink name=out sync=false", args.get("socket", ""));
            by_pipeline(args, gst::parse::launch(&pipeline).unwrap().downcast().unwrap(), "reader");
        }
        _ => bus(args),
    }
}

/// Decode the clip in this process, as each consumer does without the bus.
pub fn decode(args: &Args) {
    let tail = "appsink name=out sync=true max-buffers=1";
    let p = clip::decode_into(&args.get("clip", ""), &args.get("decoder", "avdec_h264"), tail);
    by_pipeline(args, p, "decode");
}

fn bus(args: &Args) {
    let (t0, t1) = (args.num("t0", 0), args.num("t1", 0));
    let stall = Duration::from_millis(args.num("stall-ms", 0));
    let reg = Registry::new(args.get("dir", "")).unwrap();
    let name: BusName = args.get("name", "camera:bench").parse().unwrap();
    let tally = Arc::new(Mutex::new(Tally::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let (t, s) = (tally.clone(), stop.clone());
    let worker = std::thread::spawn(move || {
        let mut sub = loop {
            match Subscriber::connect(&reg, &name) {
                Ok(sub) => break sub,
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        let mut sink = 0u64;
        while !s.load(Relaxed) {
            let Ok(Some(f)) = sub.next(Duration::from_millis(100)) else { continue };
            let now = monotonic_ns();
            sink = sink.wrapping_add(clip::touch(f.data()));
            if (t0..t1).contains(&now) {
                let mut t = t.lock().unwrap();
                t.frames += 1;
                t.skipped += f.skipped();
                t.latency_us.push((now - f.captured_ns()) / 1000);
            }
            std::thread::sleep(stall);
        }
        sink
    });
    let (cpu, rss) = cpu_over(t0, t1);
    stop.store(true, Relaxed);
    let _ = worker.join();
    let mut t = tally.lock().unwrap();
    let mut lat = std::mem::take(&mut t.latency_us);
    report(&[
        ("role", "reader".into()),
        ("cpu", format!("{cpu:.1}")),
        ("rss_mb", format!("{}", rss >> 20)),
        ("frames", t.frames.to_string()),
        ("skipped", t.skipped.to_string()),
        ("stalled", (!stall.is_zero()).to_string()),
        ("p50_us", quantile(&mut lat, 0.5).to_string()),
        ("p99_us", quantile(&mut lat, 0.99).to_string()),
        ("max_us", quantile(&mut lat, 1.0).to_string()),
    ]);
}

/// Count and touch every buffer an appsink named `out` gets, and measure.
fn by_pipeline(args: &Args, p: gst::Pipeline, role: &str) {
    let (t0, t1) = (args.num("t0", 0), args.num("t1", 0));
    let frames = Arc::new(AtomicU64::new(0));
    let sink = p.by_name("out").unwrap().downcast::<gst_app::AppSink>().unwrap();
    let f = frames.clone();
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |s| {
                let sample = s.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let map = sample.buffer().unwrap().map_readable().unwrap();
                std::hint::black_box(clip::touch(&map));
                f.fetch_add(1, Relaxed);
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    p.set_state(gst::State::Playing).expect("the reader pipeline would not play");
    sleep_until(t0);
    let f0 = frames.load(Relaxed);
    let (cpu, rss) = cpu_over(t0, t1);
    report(&[
        ("role", role.into()),
        ("cpu", format!("{cpu:.1}")),
        ("rss_mb", format!("{}", rss >> 20)),
        ("frames", (frames.load(Relaxed) - f0).to_string()),
        ("stalled", (args.num("stall-ms", 0) > 0).to_string()),
    ]);
    p.set_state(gst::State::Null).unwrap();
}
