//! The owner: decode the clip once and publish every frame, on the frame bus
//! (an appsink callback into `Publisher::push_buffer`) or on `unixfdsink`.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

use godwinmix_framebus::gst::layout_of;
use godwinmix_framebus::{BusName, Publisher, PublisherOptions, Registry};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

use crate::measure::{cpu_over, report, sleep_until};
use crate::{clip, Args};

pub fn run(args: &Args) {
    let (t0, t1) = (args.num("t0", 0), args.num("t1", 0));
    let clip = args.get("clip", "");
    let decoder = args.get("decoder", "avdec_h264");
    let format = args.get("format", "NV12");
    let convert = format!("videoconvert ! video/x-raw,format={format}");
    let frames = Arc::new(AtomicU64::new(0));
    let dropped = Arc::new(AtomicU64::new(0));
    let pipeline = match args.get("mechanism", "bus").as_str() {
        "unixfd" => {
            let tail = format!("{convert} ! identity name=count ! unixfdsink socket-path={} sync=true", args.get("socket", ""));
            let p = clip::decode_into(&clip, &decoder, &tail);
            count_on(&p, "count", frames.clone());
            p
        }
        _ => {
            let tail = format!("{convert} ! appsink name=out sync=true max-buffers=1");
            let p = clip::decode_into(&clip, &decoder, &tail);
            publish_from(&p, args, frames.clone(), dropped.clone());
            p
        }
    };
    pipeline.set_state(gst::State::Playing).expect("the owner pipeline would not play");
    sleep_until(t0);
    let (f0, d0) = (frames.load(Relaxed), dropped.load(Relaxed));
    let (cpu, rss) = cpu_over(t0, t1);
    let (f1, d1) = (frames.load(Relaxed), dropped.load(Relaxed));
    report(&[
        ("role", "owner".into()),
        ("cpu", format!("{cpu:.1}")),
        ("rss_mb", format!("{}", rss >> 20)),
        ("frames", format!("{}", f1 - f0)),
        ("dropped", format!("{}", d1 - d0)),
    ]);
    sleep_until(args.num("until", t1));
    pipeline.set_state(gst::State::Null).unwrap();
}

/// Count buffers leaving element `name`.
fn count_on(p: &gst::Pipeline, name: &str, frames: Arc<AtomicU64>) {
    let pad = p.by_name(name).unwrap().static_pad("src").unwrap();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        frames.fetch_add(1, Relaxed);
        gst::PadProbeReturn::Ok
    });
}

/// Publish every sample the appsink gets. The publisher is made on the first
/// sample, when the caps are known.
fn publish_from(p: &gst::Pipeline, args: &Args, frames: Arc<AtomicU64>, dropped: Arc<AtomicU64>) {
    let reg = Registry::new(args.get("dir", "")).unwrap();
    let name: BusName = args.get("name", "camera:bench").parse().unwrap();
    let opts = PublisherOptions {
        max_readers: args.num("max-readers", 8) as usize,
        leases_per_reader: args.num("leases", 3) as usize,
        checksum: args.num("checksum", 0) == 1,
    };
    let publisher: Mutex<Option<Publisher>> = Mutex::new(None);
    let sink = p.by_name("out").unwrap().downcast::<gst_app::AppSink>().unwrap();
    let callbacks = gst_app::AppSinkCallbacks::builder()
        .new_sample(move |s| {
            let sample = s.pull_sample().map_err(|_| gst::FlowError::Eos)?;
            let info = gst_video::VideoInfo::from_caps(sample.caps().unwrap()).unwrap();
            let mut guard = publisher.lock().unwrap();
            let p = guard.get_or_insert_with(|| {
                Publisher::create(&reg, &name, layout_of(&info).unwrap(), opts.clone()).unwrap()
            });
            match p.push_buffer(sample.buffer().unwrap(), &info) {
                Ok(true) => frames.fetch_add(1, Relaxed),
                _ => dropped.fetch_add(1, Relaxed),
            };
            Ok(gst::FlowSuccess::Ok)
        })
        .build();
    sink.set_callbacks(callbacks);
}
