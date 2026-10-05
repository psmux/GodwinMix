//! Choosing the camera and making its source element.
//!
//! First choice is GStreamer's own device provider, which knows what this
//! platform's element calls the property that picks a device and what value it
//! wants. Second choice, for an operator who forced an element or a machine
//! whose provider is missing, is the element by name with the id offered to
//! every property that might take it.
//!
//! On Windows the first try goes through Kernel Streaming. Measured on a
//! laptop with a USB2.0 FHD UVC WebCam on 2026-10-05: listing the cameras
//! through Kernel Streaming alone took 40 ms against 2.4 s for Media
//! Foundation's first probe, and 30 frames of 1080p took 0.8 to 1.1 s through
//! `ksvideosrc` against 3.6 s through `mfvideosrc`. With another app holding
//! the camera, `ksvideosrc` said "device already in use" after 0.4 s and
//! `mfvideosrc` said "Internal data stream error" after 6.9 s. Media
//! Foundation stays as the fallback, for a camera Kernel Streaming cannot see
//! and for the day GStreamer drops `ksvideosrc`, which it has marked
//! deprecated.

use gstreamer as gst;
use gstreamer::prelude::*;

use godwinmix_capture_common::{devices, elements};

use crate::settings::Settings;

/// The capture element, whatever factory it turned out to be.
pub const SOURCE: &str = "gmx-src";

/// The capture elements to try by name, in order, when no device provider
/// lists a camera.
///
/// Linux is `v4l2src` and nothing else; every camera on Linux is a V4L2
/// device. macOS is AVFoundation. Windows leads with Kernel Streaming and
/// falls back to Media Foundation, for the reasons in the module comment, and
/// because `mfvideosrc` has an open startup bug (gstreamer#2748) that leaves
/// some cameras never producing a first frame.
pub const CANDIDATES: &[&str] = if cfg!(target_os = "linux") {
    &["v4l2src"]
} else if cfg!(target_os = "macos") {
    &["avfvideosrc"]
} else if cfg!(target_os = "windows") {
    &["ksvideosrc", "mfvideosrc"]
} else {
    &["videotestsrc"]
};

/// Which way to look for the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Kernel Streaming alone, on Windows. Anywhere else the same as `Monitor`.
    Fast,
    /// Every provider on the machine, through the device monitor.
    Monitor,
}

/// The camera a source opens, with what it said it can do.
pub struct Chosen {
    pub element: gst::Element,
    /// Every mode the device lists. None for a forced element.
    pub caps: Option<gst::Caps>,
    pub sizes: Vec<(u32, u32)>,
    /// The element it was opened with, for the log and the fallback.
    pub via: String,
}

/// Find the camera the settings name and make its element.
pub fn choose(settings: &Settings, route: Route) -> Result<Chosen, String> {
    godwinmix_capture_common::init()?;
    if !settings.element.is_empty() {
        let element = by_factory(&settings.element, &settings.device)?;
        return Ok(chosen(element, None, Vec::new()));
    }
    if route == Route::Fast {
        if let Some(found) = kernel_streaming(&settings.device) {
            return Ok(found);
        }
    }
    match devices::find(devices::CAMERA, &settings.device) {
        Ok(found) => Ok(chosen(found.element(SOURCE)?, found.caps(), found.sizes())),
        Err(from_monitor) => {
            // The monitor works and this is not one of its devices. It has
            // said which ones there are, and no element by name will do better.
            if devices::lists_any(devices::CAMERA) {
                return Err(from_monitor);
            }
            let factory = elements::require(
                "capturing a camera",
                CANDIDATES,
                "Install the GStreamer plugin that carries it \
                 (gstreamer1.0-plugins-good on Debian, gst-plugins-good in Homebrew).",
            )
            .map_err(|missing| format!("{from_monitor} {missing}"))?;
            Ok(chosen(
                by_factory(factory, &settings.device)?,
                None,
                Vec::new(),
            ))
        }
    }
}

fn chosen(element: gst::Element, caps: Option<gst::Caps>, sizes: Vec<(u32, u32)>) -> Chosen {
    let via = element
        .factory()
        .map(|f| f.name().to_string())
        .unwrap_or_default();
    Chosen {
        element,
        caps,
        sizes,
        via,
    }
}

/// The camera through Kernel Streaming, when this is Windows and `wanted`
/// names one it lists. None sends the caller to the device monitor, which is
/// also what happens for an empty id or an index: both mean "in the order the
/// monitor lists them", and only the monitor knows that order.
fn kernel_streaming(wanted: &str) -> Option<Chosen> {
    if !cfg!(windows) || wanted.is_empty() || wanted.parse::<usize>().is_ok() {
        return None;
    }
    devices::from_provider("ksdeviceprovider", devices::CAMERA)
        .into_iter()
        .find_map(|found| {
            let element = found.element(SOURCE).ok()?;
            let path = element
                .find_property("device-path")
                .and_then(|_| element.property::<Option<String>>("device-path"))
                .unwrap_or_default();
            (same_camera(wanted, &path) || found.name == wanted)
                .then(|| chosen(element, found.caps(), found.sizes()))
        })
}

/// Whether two Windows device paths name one camera. Media Foundation and
/// Kernel Streaming give the same instance path with a different interface
/// class after it: `...&0&0000#{e5323777-...}\global` against
/// `...&0&0000#{6994ad05-...}\global`.
pub fn same_camera(a: &str, b: &str) -> bool {
    let instance = |p: &str| {
        let p = p.to_ascii_lowercase();
        match p.rfind("#{") {
            Some(at) => p[..at].to_string(),
            None => p,
        }
    };
    !a.is_empty() && !b.is_empty() && instance(a) == instance(b)
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
    // `do-timestamp` is left alone: a capture element stamps its frames from
    // when it took them, and replacing that with a clock reading taken when
    // the buffer was pushed is a jitter `videorate` downstream then believes.
    Ok(element)
}

#[cfg(test)]
mod tests;
