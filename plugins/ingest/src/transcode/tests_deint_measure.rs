//! A measuring run, not a test: a captured broadcast decoded on NVDEC and
//! put through the scale node a headend channel gets, deinterlaced on the
//! card or, with `GMX_CUDA_DEINTERLACE=off`, on the CPU, and written as
//! y4m for scoring against a reference. At the clip's own speed, as a
//! channel runs, so the CPU it takes is what a live channel takes.
//!
//!   GMX_MEASURE_IN=clip.ts GMX_MEASURE_OUT=out.y4m \
//!     cargo test -p gmx-ingest --bin gmx-ingest measure_deinterlace -- --ignored

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;

use super::build::build;
use super::spec::NodeSpec;

#[test]
#[ignore = "needs a card, a clip and GMX_MEASURE_IN/OUT"]
fn measure_deinterlace() {
    let (Ok(input), Ok(output)) = (std::env::var("GMX_MEASURE_IN"), std::env::var("GMX_MEASURE_OUT")) else {
        panic!("set GMX_MEASURE_IN and GMX_MEASURE_OUT");
    };
    gmx_netkit::init().unwrap();
    let raw = json!({"kind": "scale", "width": 1280, "height": 720, "fps": [30000, 1001], "fit": "pad", "border": 0xff00_0000u64,
        "deinterlace": "auto", "method": "yadif", "rate": false, "memory": "cuda"});
    let node = NodeSpec { id: "scale:measure".into(), kind: "scale".into(), input: "decode".into(), raw };
    let built = build(&node).unwrap();
    // Paced to the clock after the decoder, as a live input is: the scale
    // starts with a leaky queue, which drops what comes faster.
    // The demuxer's pad comes late: linked inside one launched pipeline, not
    // through a ghost pad, which would leave it unlinked.
    let launch = format!("filesrc location={input} ! tsdemux ! mpegvideoparse ! nvmpeg2videodec ! identity sync=true name=decoded");
    let pipeline = gst::parse::launch(&launch).unwrap().downcast::<gst::Pipeline>().unwrap();
    let decoded = pipeline.by_name("decoded").unwrap();
    let back = gst::parse::bin_from_description(&format!("cudadownload ! videoconvert ! video/x-raw,format=I420 ! y4menc ! filesink location={output}"), true).unwrap();
    pipeline.add(&back).unwrap();
    pipeline.add_many(built.elements.iter()).unwrap();
    gst::Element::link_many(built.elements.iter()).unwrap();
    decoded.link(built.head.as_ref().unwrap()).unwrap();
    built.tee.as_ref().unwrap().link(&back).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let bus = pipeline.bus().unwrap();
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            gst::MessageView::Eos(..) => break,
            gst::MessageView::Error(e) => panic!("{}: {:?}", e.error(), e.debug()),
            _ => {}
        }
    }
    pipeline.set_state(gst::State::Null).unwrap();
}
