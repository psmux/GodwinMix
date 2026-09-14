//! Building the screen capture's pipeline.
//!
//! ```text
//!  <screen> [! d3d11convert ! d3d11download] ! video/x-raw ! videoconvert !
//!           videoscale ! videorate ! video/x-raw,format=I420,<canvas> !
//!           queue ! <transport>
//! ```
//!
//! The two `d3d11` elements are only in the chain on Windows and only when the
//! Direct3D capture element was the one chosen: it hands back frames in GPU
//! memory that `videoconvert` cannot read, and downloading them once is the
//! whole fix.
//!
//! Every capture element spells the same idea differently, so the settings are
//! applied by offering each value to every property name that might take it
//! and letting the ones that do not decline. It is shorter than a table per
//! platform and it does not go stale when an element gains a property.

use gstreamer as gst;
use gstreamer::prelude::*;

use godwinmix_capture_common::{capture, elements, wiring};
use godwinmix_sdk::wire::{Canvas, Transport};

use crate::settings::{Region, Settings};

/// The element the source is attached to.
const HEAD: &str = "gmx-head";
/// The capture element.
pub const SOURCE: &str = "gmx-src";

/// The capture elements to try, in order, on this platform.
///
/// Linux leads with PipeWire, which is the only way in on Wayland and goes
/// through `xdg-desktop-portal` so the person at the keyboard agrees to it.
/// `ximagesrc` is the X11 fallback. macOS is AVFoundation with
/// `capture-screen`. Windows is Desktop Duplication through Direct3D 11, with
/// the older DXGI and GDI elements behind it for a machine whose driver will
/// not do it.
pub const CANDIDATES: &[&str] = if cfg!(target_os = "linux") {
    &["pipewiresrc", "ximagesrc"]
} else if cfg!(target_os = "macos") {
    &["avfvideosrc"]
} else if cfg!(target_os = "windows") {
    &[
        "d3d11screencapturesrc",
        "dxgiscreencapsrc",
        "gdiscreencapsrc",
    ]
} else {
    &["videotestsrc"]
};

/// The chain from the capture element to the canvas caps, without its sink.
pub fn chain(factory: &str, canvas: Canvas) -> String {
    let mut parts: Vec<String> = Vec::new();
    if factory.starts_with("d3d11") && elements::exists("d3d11download") {
        parts.push("d3d11convert".into());
        parts.push("d3d11download".into());
    }
    // System memory, explicitly. A macOS screen would otherwise hand back GL
    // textures and a Direct3D one would hand back GPU surfaces.
    parts.push("capsfilter caps=video/x-raw".into());
    parts.push("videoconvert".into());
    parts.push("videoscale".into());
    parts.push("videorate".into());
    parts.push(wiring::canvas_video_caps(
        canvas.width,
        canvas.height,
        canvas.fps,
    ));
    parts[0] = format!("{} name={HEAD}", parts[0]);
    parts.join(" ! ")
}

/// The whole pipeline, with the capture attached and the addresses bound.
pub fn build(
    settings: &Settings,
    canvas: Canvas,
    transport: Transport,
    address: &str,
) -> Result<gst::Pipeline, String> {
    let factory = factory_for(settings)?;
    let description = wiring::Wiring::video_only(chain(&factory, canvas)).description(transport)?;
    let pipeline = capture::build(&description)?;
    wiring::bind(&pipeline, transport, address)?;
    let source = open(&factory, settings)?;
    attach(&pipeline, &source)?;
    Ok(pipeline)
}

/// Which element this machine will use.
///
/// A forced element is returned as it stands; whether this machine has it is
/// `open`'s question to answer, and its error names the element, which is what
/// the operator needs to read.
pub fn factory_for(settings: &Settings) -> Result<String, String> {
    if !settings.element.is_empty() {
        return Ok(settings.element.clone());
    }
    // On Linux, `pipewiresrc` is only usable with a node id from a portal
    // session. Without one, fall past it to X11 rather than opening a capture
    // of nothing. The README says how to get a node id and why it is not asked
    // for automatically yet.
    let usable: Vec<&'static str> = CANDIDATES
        .iter()
        .copied()
        .filter(|c| *c != "pipewiresrc" || !settings.node_id.is_empty())
        .collect();
    elements::require(
        "capturing a screen",
        &usable,
        "On Linux install gstreamer1.0-plugins-good for ximagesrc, or gstreamer1.0-pipewire \
         and xdg-desktop-portal for Wayland. On Windows install the GStreamer runtime's bad \
         plugins.",
    )
    .map(str::to_string)
}

/// Make the capture element and put the settings on it.
pub fn open(factory: &str, settings: &Settings) -> Result<gst::Element, String> {
    let element = gst::ElementFactory::make(factory)
        .name(SOURCE)
        .build()
        .map_err(|e| format!("this machine has no '{factory}' element: {e}"))?;
    // AVFoundation's video source is a camera unless it is told otherwise.
    elements::set_flag(&element, "capture-screen", true);
    for name in [
        "capture-screen-cursor",
        "show-pointer",
        "show-cursor",
        "cursor",
    ] {
        elements::set_flag(&element, name, settings.show_cursor);
    }
    for name in ["device-index", "monitor-index", "monitor"] {
        elements::set_number(&element, name, settings.monitor as i64);
    }
    if !settings.display.is_empty() {
        elements::set_text(&element, "display-name", &settings.display);
    }
    if !settings.node_id.is_empty() {
        // `pipewiresrc` calls it `path`, and it is the node the portal granted.
        elements::set_text(&element, "path", &settings.node_id);
    }
    if let Some(region) = settings.region {
        crop(&element, region);
    }
    // `ximagesrc` with damage events sends a frame only when something moved,
    // which is not a frame rate. The canvas wants one every time.
    elements::set_flag(&element, "use-damage", false);
    Ok(element)
}

/// Put a region on whichever of the four spellings this element uses.
fn crop(element: &gst::Element, region: Region) {
    let (x, y) = (region.x as i64, region.y as i64);
    let (w, h) = (region.width as i64, region.height as i64);
    // AVFoundation.
    elements::set_number(element, "screen-crop-x", x);
    elements::set_number(element, "screen-crop-y", y);
    elements::set_number(element, "screen-crop-width", w);
    elements::set_number(element, "screen-crop-height", h);
    // Direct3D 11.
    elements::set_number(element, "crop-x", x);
    elements::set_number(element, "crop-y", y);
    elements::set_number(element, "crop-width", w);
    elements::set_number(element, "crop-height", h);
    // X11 and GDI, which take corners rather than a size.
    elements::set_number(element, "startx", x);
    elements::set_number(element, "starty", y);
    elements::set_number(element, "endx", x + w - 1);
    elements::set_number(element, "endy", y + h - 1);
}

fn attach(pipeline: &gst::Pipeline, source: &gst::Element) -> Result<(), String> {
    let head = pipeline
        .by_name(HEAD)
        .ok_or_else(|| format!("the pipeline has no '{HEAD}' to attach the capture to"))?;
    pipeline
        .add(source)
        .map_err(|e| format!("could not put the capture in the pipeline: {e}"))?;
    source.link(&head).map_err(|e| {
        format!(
            "the capture and the pipeline would not agree on a format: {e}. Clear the \
             `region` setting and try again."
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
        let chain = chain("ximagesrc", canvas());
        assert!(chain.contains("format=I420"), "{chain}");
        assert!(
            chain.contains("width=1280,height=720,framerate=30/1"),
            "{chain}"
        );
        assert!(
            chain.starts_with(&format!("capsfilter caps=video/x-raw name={HEAD}")),
            "{chain}"
        );
        assert!(!chain.contains("d3d11"), "{chain}");
    }

    #[test]
    fn a_direct3d_capture_downloads_its_frames_before_anything_reads_them() {
        godwinmix_capture_common::init().unwrap();
        let chain = chain("d3d11screencapturesrc", canvas());
        if elements::exists("d3d11download") {
            assert!(
                chain.starts_with(&format!("d3d11convert name={HEAD}")),
                "{chain}"
            );
            assert!(chain.contains("d3d11download"), "{chain}");
        } else {
            assert!(
                !chain.contains("d3d11download"),
                "not on this platform: {chain}"
            );
        }
    }

    #[test]
    fn this_platform_has_a_capture_element_named_for_it() {
        if cfg!(target_os = "linux") {
            assert_eq!(CANDIDATES, &["pipewiresrc", "ximagesrc"]);
        }
        if cfg!(target_os = "macos") {
            assert_eq!(CANDIDATES, &["avfvideosrc"]);
        }
        if cfg!(target_os = "windows") {
            assert_eq!(
                CANDIDATES,
                &[
                    "d3d11screencapturesrc",
                    "dxgiscreencapsrc",
                    "gdiscreencapsrc"
                ]
            );
        }
    }

    #[test]
    fn pipewire_is_skipped_until_a_portal_has_granted_a_node() {
        godwinmix_capture_common::init().unwrap();
        if !elements::exists("pipewiresrc") {
            return;
        }
        let without = Settings::from(&json!({}));
        assert_ne!(factory_for(&without).unwrap(), "pipewiresrc");
        let with = Settings::from(&json!({"node_id": "42"}));
        assert_eq!(factory_for(&with).unwrap(), "pipewiresrc");
    }

    #[test]
    fn a_test_pattern_builds_a_whole_pipeline_on_any_machine() {
        godwinmix_capture_common::init().unwrap();
        let settings = Settings::from(&json!({"element": "videotestsrc"}));
        let pipeline = build(&settings, canvas(), Transport::Container, "")
            .expect("a test pattern builds everywhere");
        assert!(pipeline.by_name(SOURCE).is_some());
    }

    #[test]
    fn a_region_reaches_whichever_property_the_element_calls_it() {
        godwinmix_capture_common::init().unwrap();
        let settings = Settings::from(&json!({"element": "videotestsrc", "region": "0,0,320,240"}));
        // videotestsrc has none of the four spellings, and setting a property
        // that is not there must be a no-op rather than a crash.
        build(&settings, canvas(), Transport::Container, "").expect("it builds regardless");
    }
}
