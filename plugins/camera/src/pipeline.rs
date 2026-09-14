//! Building the camera's pipeline: the source element, then the chain that
//! turns whatever it produces into the canvas contract.
//!
//! ```text
//!  <camera> ! caps ! decodebin ! videoconvert ! videoscale ! videorate !
//!            video/x-raw,format=I420,<canvas> ! queue ! <transport>
//! ```
//!
//! `decodebin` is in there because a camera that will only give 1080p as
//! Motion JPEG or H.264 is common, and because a camera that gives raw frames
//! links straight through it at no cost. `videorate` is in there because the
//! canvas has one frame rate and a camera has another, and the compositor
//! should never be the thing that notices.

use gstreamer as gst;
use gstreamer::prelude::*;

use godwinmix_capture_common::{capture, devices, elements, wiring};
use godwinmix_sdk::wire::{Canvas, Transport};

use crate::settings::Settings;

/// The element the source is attached to, and where the chain begins.
const HEAD: &str = "gmx-devcaps";
/// The capture element, whatever factory it turned out to be.
pub const SOURCE: &str = "gmx-src";

/// The capture elements to try, in order, on this platform.
///
/// Linux is `v4l2src` and nothing else; every camera on Linux is a V4L2
/// device. macOS is AVFoundation. Windows leads with Media Foundation and
/// falls back to Kernel Streaming, because `mfvideosrc` has an open startup
/// bug (gstreamer#2748) that leaves some cameras never producing a first
/// frame, and `ksvideosrc` is the older path that works.
pub const CANDIDATES: &[&str] = if cfg!(target_os = "linux") {
    &["v4l2src"]
} else if cfg!(target_os = "macos") {
    &["avfvideosrc"]
} else if cfg!(target_os = "windows") {
    &["mfvideosrc", "ksvideosrc"]
} else {
    &["videotestsrc"]
};

/// The chain from the device caps to the canvas caps, without its sink.
pub fn chain(settings: &Settings, canvas: Canvas) -> String {
    format!(
        "capsfilter name={HEAD} caps=\"{}\" ! decodebin name=gmx-decode ! \
         videoconvert ! videoscale ! videorate ! {}",
        settings.device_caps().replace('"', ""),
        wiring::canvas_video_caps(canvas.width, canvas.height, canvas.fps)
    )
}

/// The whole pipeline, with the camera attached and the addresses bound.
pub fn build(
    settings: &Settings,
    canvas: Canvas,
    transport: Transport,
    address: &str,
) -> Result<gst::Pipeline, String> {
    let description = wiring::Wiring::video_only(chain(settings, canvas)).description(transport)?;
    let pipeline = capture::build(&description)?;
    wiring::bind(&pipeline, transport, address)?;
    let source = open(settings)?;
    attach(&pipeline, &source)?;
    Ok(pipeline)
}

/// Open the camera the settings name.
///
/// First choice is GStreamer's own device provider, which knows what this
/// platform's element calls the property that picks a device and what value it
/// wants. Second choice, for an operator who forced an element or a machine
/// whose provider is missing, is the element by name with the id offered to
/// every property that might take it.
pub fn open(settings: &Settings) -> Result<gst::Element, String> {
    if !settings.element.is_empty() {
        return by_factory(&settings.element, &settings.device);
    }
    match devices::find(devices::CAMERA, &settings.device) {
        Ok(found) => found.element(SOURCE),
        Err(from_monitor) => {
            let factory = elements::require(
                "capturing a camera",
                CANDIDATES,
                "Install the GStreamer plugin that carries it \
                 (gstreamer1.0-plugins-good on Debian, gst-plugins-good in Homebrew).",
            )
            .map_err(|missing| format!("{from_monitor} {missing}"))?;
            by_factory(factory, &settings.device)
        }
    }
}

fn by_factory(factory: &str, device: &str) -> Result<gst::Element, String> {
    let element = gst::ElementFactory::make(factory)
        .name(SOURCE)
        .build()
        .map_err(|e| format!("this machine has no '{factory}' element: {e}"))?;
    if !device.is_empty() && elements::point_at(&element, device).is_none() {
        return Err(format!(
            "'{factory}' has no property that takes a device id, so '{device}' cannot be \
             selected with it. Clear the `element` setting and let the plugin choose."
        ));
    }
    // A camera is live: its frames are worth what they were worth when they
    // were taken, and a pipeline that tried to catch up would show old ones.
    elements::set_flag(&element, "is-live", true);
    elements::set_flag(&element, "do-timestamp", true);
    Ok(element)
}

fn attach(pipeline: &gst::Pipeline, source: &gst::Element) -> Result<(), String> {
    let head = pipeline
        .by_name(HEAD)
        .ok_or_else(|| format!("the pipeline has no '{HEAD}' to attach the camera to"))?;
    pipeline
        .add(source)
        .map_err(|e| format!("could not put the camera in the pipeline: {e}"))?;
    source.link(&head).map_err(|e| {
        format!(
            "the camera and the pipeline would not agree on a format: {e}. Clear the \
             `resolution` and `framerate` settings and let the camera choose."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn canvas() -> Canvas {
        Canvas::new(1280, 720, 30)
    }

    #[test]
    fn the_chain_ends_at_the_canvas_contract() {
        let chain = chain(&Settings::from(&json!({})), canvas());
        assert!(chain.contains("format=I420"), "{chain}");
        assert!(
            chain.contains("width=1280,height=720,framerate=30/1"),
            "{chain}"
        );
        assert!(chain.contains("videorate"), "{chain}");
        assert!(
            chain.starts_with(&format!("capsfilter name={HEAD}")),
            "{chain}"
        );
    }

    #[test]
    fn this_platform_has_a_capture_element_named_for_it() {
        assert!(!CANDIDATES.is_empty());
        if cfg!(target_os = "macos") {
            assert_eq!(CANDIDATES, &["avfvideosrc"]);
        }
        if cfg!(target_os = "windows") {
            assert_eq!(
                CANDIDATES,
                &["mfvideosrc", "ksvideosrc"],
                "the ks fallback must stay"
            );
        }
        if cfg!(target_os = "linux") {
            assert_eq!(CANDIDATES, &["v4l2src"]);
        }
    }

    #[test]
    fn a_forced_element_that_does_not_exist_names_itself() {
        godwinmix_capture_common::init().unwrap();
        let settings = Settings::from(&json!({"element": "v4l2src"}));
        if elements::exists("v4l2src") {
            return; // on Linux this is the real path and is tested elsewhere
        }
        let err = open(&settings).expect_err("not on this platform");
        assert!(err.contains("v4l2src"), "{err}");
    }

    #[test]
    fn a_test_pattern_builds_a_whole_pipeline_on_any_machine() {
        godwinmix_capture_common::init().unwrap();
        let settings = Settings::from(&json!({"element": "videotestsrc"}));
        let pipeline = build(&settings, canvas(), Transport::Container, "")
            .expect("a test pattern builds everywhere");
        assert!(pipeline.by_name(SOURCE).is_some());
        assert!(pipeline.by_name("gmx-video-queue").is_some());
    }

    #[test]
    fn a_socket_transport_with_no_address_is_refused_before_anything_opens() {
        godwinmix_capture_common::init().unwrap();
        let settings = Settings::from(&json!({"element": "videotestsrc"}));
        let err = build(&settings, canvas(), Transport::Unixfd, "").expect_err("no address");
        assert!(err.contains("container"), "{err}");
    }
}
