//! A moving test picture with the wall clock burned into every frame, to the
//! millisecond, as MPEG-TS on stdout. For measuring latency end to end: add
//! it as an `exec:` source, put it on air, and compare the time in the
//! picture a player shows with the clock beside the player.
//!
//! ```text
//! cargo run -p godwinmix-core --example clock_ts > /dev/null
//! source.add {id: "clock", uri: "exec:/path/to/target/debug/examples/clock_ts"}
//! ```
//!
//! `clockoverlay` would do this if its format had milliseconds; it does not,
//! so a probe on the overlay's input sets the text from `SystemTime` as each
//! frame arrives from the live source, which is when it was made.

use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;

fn main() -> Result<()> {
    gst::init()?;
    let pipeline = gst::parse::launch(
        "videotestsrc is-live=true pattern=ball ! video/x-raw,width=1280,height=720,framerate=30/1 \
         ! textoverlay name=clock font-desc=\"Monospace Bold 64\" valignment=top halignment=left \
           shaded-background=true \
         ! videoconvert ! x264enc tune=zerolatency speed-preset=ultrafast key-int-max=60 bitrate=3000 \
         ! h264parse ! mux. \
         audiotestsrc is-live=true wave=ticks ! audioconvert ! avenc_aac ! aacparse ! mux. \
         mpegtsmux name=mux alignment=7 ! fdsink fd=1 sync=false",
    )?
    .downcast::<gst::Pipeline>()
    .map_err(|_| anyhow::anyhow!("not a pipeline"))?;
    let overlay = pipeline.by_name("clock").expect("the overlay is named clock");
    let sink = overlay.static_pad("video_sink").expect("textoverlay has a video sink");
    let weak = overlay.downgrade();
    sink.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        if let Some(o) = weak.upgrade() {
            o.set_property("text", now());
        }
        gst::PadProbeReturn::Ok
    });
    pipeline.set_state(gst::State::Playing)?;
    let bus = pipeline.bus().expect("a pipeline has a bus");
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            gst::MessageView::Eos(..) => break,
            gst::MessageView::Error(e) => {
                eprintln!("clock_ts: {}", e.error());
                break;
            }
            _ => {}
        }
    }
    pipeline.set_state(gst::State::Null)?;
    Ok(())
}

/// The local time of day, `HH:MM:SS.mmm`, in UTC.
fn now() -> String {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let (s, ms) = (d.as_secs() % 86_400, d.subsec_millis());
    format!("{:02}:{:02}:{:02}.{ms:03}", s / 3600, (s / 60) % 60, s % 60)
}
