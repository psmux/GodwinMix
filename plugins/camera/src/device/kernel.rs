//! The Windows Kernel Streaming route: one provider asked, no device monitor.

use gstreamer::prelude::*;

use godwinmix_capture_common::devices;

use super::{chosen, Chosen, SOURCE};

/// The camera through Kernel Streaming, when this is Windows and `wanted`
/// names one it lists. None sends the caller to the device monitor, which is
/// also what happens for an empty id or an index: both mean "in the order the
/// monitor lists them", and only the monitor knows that order.
pub fn kernel_streaming(wanted: &str) -> Option<Chosen> {
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
