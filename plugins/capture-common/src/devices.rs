//! Finding cameras and sound cards through GStreamer's own `DeviceMonitor`.
//!
//! Every platform names its devices differently: `/dev/video0` on Linux, a
//! zero based index on macOS, a long `\\?\usb#vid_...` path on Windows. A
//! plugin that spelled any of those itself would work on one platform and
//! guess on the other two.
//!
//! `DeviceMonitor` already knows. It lists what is plugged in, and
//! `Device::create_element` hands back a source element already pointed at
//! that device, with the right property set to the right value for whichever
//! element this platform uses. So the plugins here never set `device`,
//! `device-index` or `device-path` themselves; they ask for a device by the id
//! the operator chose and let GStreamer wire it.
//!
//! The id in the settings is the device's most stable property (`device.path`
//! on Linux, `avf.unique_id` on macOS, and so on), and the operator may also
//! type the display name or a plain index. All three are matched here, so a
//! configuration copied from one machine to another with the same camera
//! keeps working.

use gstreamer as gst;
use gstreamer::prelude::*;

use godwinmix_sdk::wire::Candidate;

/// The device classes a camera lives under.
pub const CAMERA: &[&str] = &["Video/Source"];
/// The device classes a microphone or a line input lives under.
pub const MICROPHONE: &[&str] = &["Audio/Source"];

/// The properties that identify a device, in the order they are tried.
///
/// The first one present becomes the id stored in the settings. They are
/// stable across reboots on their platform, which the display name is not when
/// two identical cameras are plugged in.
const ID_KEYS: &[&str] = &[
    "device.path",   // v4l2, pipewire
    "avf.unique_id", // avfvideosrc
    "device.strid",  // mediafoundation, ks
    "device.serial", // pipewire
    "unique-id",     // osxaudiosrc, wasapi2
    "device.id",
    "object.path",
];

/// One device, as this crate reports it.
#[derive(Debug)]
pub struct Found {
    /// What goes in the `device` setting.
    pub id: String,
    /// What a person sees in the picker.
    pub name: String,
    /// `Video/Source`, `Audio/Source`, and so on.
    pub class: String,
    /// The capture API behind it, when the platform says: `avf`, `v4l2`,
    /// `wasapi2`, `pipewire`.
    pub api: Option<String>,
    /// The largest size the device advertises, as `width x height`, when it
    /// advertises one.
    pub best_size: Option<(i32, i32)>,
    device: gst::Device,
}

impl Found {
    /// A source element already pointed at this device.
    pub fn element(&self, name: &str) -> Result<gst::Element, String> {
        self.device
            .create_element(Some(name))
            .map_err(|e| format!("could not open '{}': {e}", self.name))
    }

    /// The candidate a `device` provide answers `discover` with.
    pub fn candidate(&self, provide: &str) -> Candidate {
        Candidate {
            kind: provide.to_string(),
            name: self.name.clone(),
            params: serde_json::json!({ "device": self.id, "label": self.name }),
            // Something the operating system is telling us about is really
            // there. The number is here for finders that guess; this one does
            // not.
            confidence: 1.0,
        }
    }
}

/// Everything plugged in under these classes, in the order the platform gives.
///
/// The monitor is started and stopped inside the call: a plugin that held one
/// open would be told about every hot plug for the life of the process and has
/// nothing to do with the news.
pub fn list(classes: &[&str]) -> Result<Vec<Found>, String> {
    crate::init()?;
    let monitor = gst::DeviceMonitor::new();
    for class in classes {
        monitor.add_filter(Some(class), None);
    }
    // A monitor whose filters match no provider on this machine refuses to
    // start at all. That is not a failure, it is the answer: there are none.
    if monitor.start().is_err() {
        return Ok(Vec::new());
    }
    let devices = monitor.devices();
    monitor.stop();
    Ok(devices.into_iter().map(describe).collect())
}

fn describe(device: gst::Device) -> Found {
    let name = device.display_name().to_string();
    let properties = device.properties();
    let value = |key: &str| {
        properties
            .as_ref()
            .and_then(|s| s.get::<String>(key).ok())
            .filter(|v| !v.is_empty())
    };
    let id = ID_KEYS
        .iter()
        .find_map(|k| value(k))
        .unwrap_or_else(|| name.clone());
    Found {
        id,
        name,
        class: device.device_class().to_string(),
        api: value("device.api"),
        best_size: largest_size(&device),
        device,
    }
}

/// The largest width and height in the device's caps, for the picker's detail
/// line. Ranges are read at their maximum, which is what a camera can do.
fn largest_size(device: &gst::Device) -> Option<(i32, i32)> {
    let caps = device.caps()?;
    let mut best: Option<(i32, i32)> = None;
    for structure in caps.iter() {
        let (Ok(w), Ok(h)) = (
            structure.get::<i32>("width"),
            structure.get::<i32>("height"),
        ) else {
            continue;
        };
        if best.is_none_or(|(bw, bh)| (w as i64 * h as i64) > (bw as i64 * bh as i64)) {
            best = Some((w, h));
        }
    }
    best
}

/// Find one device by the id, the display name, or a plain index.
///
/// An empty `wanted` takes the first device the platform lists, which is what
/// an operator who has one camera means.
pub fn find(classes: &[&str], wanted: &str) -> Result<Found, String> {
    let devices = list(classes)?;
    if devices.is_empty() {
        return Err(format!(
            "this machine has no {} that GStreamer can see. Plug one in, or check that \
             another program does not already have it open.",
            classes.join(" or ")
        ));
    }
    if wanted.is_empty() {
        return Ok(devices.into_iter().next().expect("the list is not empty"));
    }
    if let Some(found) = devices
        .iter()
        .position(|d| d.id == wanted || d.name == wanted)
    {
        return Ok(devices.into_iter().nth(found).expect("just found"));
    }
    if let Ok(index) = wanted.parse::<usize>() {
        if index < devices.len() {
            return Ok(devices.into_iter().nth(index).expect("in range"));
        }
    }
    Err(format!(
        "no device matches '{wanted}'. This machine has: {}. Put one of those in the \
         `device` setting, or leave it empty for the first one.",
        devices
            .iter()
            .map(|d| format!("'{}' ({})", d.name, d.id))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Every device under these classes as a `discover` answer.
pub fn candidates(classes: &[&str], provide: &str) -> Result<Vec<Candidate>, String> {
    Ok(list(classes)?
        .iter()
        .map(|d| d.candidate(provide))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_a_class_that_exists_nowhere_is_empty_and_not_an_error() {
        // GStreamer refuses to start a monitor with no matching provider.
        // That has to read as "none of those here", not as a broken machine.
        let found = list(&["Nonsense/Source"]).expect("an empty answer, not an error");
        assert!(found.is_empty());
    }

    #[test]
    fn asking_for_a_device_on_a_machine_with_none_names_the_class() {
        let err = find(&["Nonsense/Source"], "").expect_err("there are none");
        assert!(err.contains("Nonsense/Source"), "{err}");
    }

    #[test]
    fn a_candidate_carries_the_provide_id_and_ready_params() {
        crate::init().unwrap();
        // Any device will do; a machine with none skips the check rather than
        // failing a test suite that has nothing to do with hardware.
        let Some(device) = list(CAMERA).unwrap().into_iter().next() else {
            return;
        };
        let candidate = device.candidate("camera/source");
        assert_eq!(candidate.kind, "camera/source");
        assert!(!candidate.name.is_empty());
        assert_eq!(candidate.params["device"], device.id);
        assert_eq!(candidate.confidence, 1.0);
    }
}
