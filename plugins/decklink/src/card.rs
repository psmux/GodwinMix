//! What this machine has of DeckLink, and the capture pipeline for one input.
//!
//! ```text
//!   decklinkvideosrc ! videoconvert ! videoscale ! videorate ! <canvas video> ! <transport>
//!   decklinkaudiosrc ! audioconvert ! audioresample ! <canvas audio> ! <transport>
//! ```
//!
//! Three things have to be true before a picture arrives: GStreamer's
//! `decklink` plugin (gst-plugins-bad), Blackmagic's Desktop Video driver, and
//! a card. Each missing one is said in its own sentence, because the fix for
//! each is different.

use gstreamer as gst;
use gstreamer::prelude::*;

use godwinmix_capture_common::wiring::{self, Wiring};
use godwinmix_sdk::wire::{Canvas, Candidate, Transport};

use crate::settings::Settings;

/// Why nothing can be captured here, or `None` when the element is present.
pub fn missing_element() -> Option<String> {
    let present = ["decklinkvideosrc", "decklinkaudiosrc"].iter().all(|e| gst::ElementFactory::find(e).is_some());
    (!present).then(|| {
        "this GStreamer has no DeckLink elements. They are in gst-plugins-bad (gstreamer1.0-plugins-bad on \
         Debian and Ubuntu; the GStreamer installer from gstreamer.freedesktop.org on macOS and Windows)."
            .to_string()
    })
}

/// Every DeckLink input GStreamer's device provider can see, as `discover`
/// candidates. Empty when there is no driver or no card: the provider lists
/// only what the driver reports.
pub fn inputs() -> Vec<Candidate> {
    let monitor = gst::DeviceMonitor::new();
    monitor.add_filter(Some("Video/Source"), None);
    if monitor.start().is_err() {
        return Vec::new();
    }
    let devices = monitor.devices();
    monitor.stop();
    devices
        .iter()
        .filter_map(|d| {
            let element = d.create_element(None).ok()?;
            let factory = element.factory()?;
            (factory.name() == "decklinkvideosrc").then(|| {
                let number = element.property::<i32>("device-number");
                let name = d.display_name().to_string();
                Candidate {
                    kind: "decklink/source".into(),
                    name: name.clone(),
                    params: serde_json::json!({ "device_number": number, "label": name }),
                    confidence: 1.0,
                }
            })
        })
        .collect()
}

/// The capture chains for one input, ending at the canvas contract.
pub fn wiring(s: &Settings, canvas: Canvas) -> Wiring {
    let video = format!(
        "decklinkvideosrc name=gmx-src device-number={} connection={} mode={} ! videoconvert ! videoscale ! videorate ! {}",
        s.device_number,
        s.connection,
        s.mode,
        wiring::canvas_video_caps(canvas.width, canvas.height, canvas.fps)
    );
    let audio = s.audio.then(|| {
        format!(
            "decklinkaudiosrc device-number={} ! audioconvert ! audioresample ! {}",
            s.device_number,
            wiring::canvas_audio_caps()
        )
    });
    Wiring { video: Some(video), audio }
}

/// The whole pipeline, sinks bound to the transport the core negotiated.
pub fn build(s: &Settings, canvas: Canvas, transport: Transport, address: &str) -> Result<gst::Pipeline, String> {
    if let Some(missing) = missing_element() {
        return Err(missing);
    }
    let description = wiring(s, canvas).description(transport)?;
    let pipeline = godwinmix_capture_common::capture::build(&description)?;
    wiring::bind(&pipeline, transport, address)?;
    Ok(pipeline)
}

/// Turn GStreamer's words for a missing card into the operator's.
pub fn explain(e: &str, s: &Settings) -> String {
    let lower = e.to_lowercase();
    if lower.contains("acquire") || lower.contains("failed to") || lower.contains("not negotiated") || lower.contains("state") {
        return format!(
            "{} would not open: {e}. Check that the Blackmagic Desktop Video driver is installed and the card is seen \
             in Blackmagic Desktop Video Setup, that input {} exists, and that nothing else (another program, another \
             source) is using it.",
            s.describe(),
            s.device_number
        );
    }
    format!("{} would not open: {e}", s.describe())
}
