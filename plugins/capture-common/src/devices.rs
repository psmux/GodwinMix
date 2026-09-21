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
    /// Every picture size the device says it can deliver, widest pictures
    /// first and largest first among those, each size once.
    ///
    /// For the picker's list, and for choosing one when nobody has: see
    /// [`pick_size`]. Sizes given as ranges are left out, because a range is
    /// not something to offer in a list.
    pub fn sizes(&self) -> Vec<(u32, u32)> {
        let Some(caps) = self.device.caps() else { return Vec::new() };
        let mut out: Vec<(u32, u32)> = Vec::new();
        for structure in caps.iter() {
            let (Ok(w), Ok(h)) = (structure.get::<i32>("width"), structure.get::<i32>("height")) else {
                continue;
            };
            if w <= 0 || h <= 0 || out.contains(&(w as u32, h as u32)) {
                continue;
            }
            out.push((w as u32, h as u32));
        }
        out.sort_by(|a, b| {
            let wide = |s: &(u32, u32)| s.0 >= s.1;
            wide(b).cmp(&wide(a)).then((b.0 as u64 * b.1 as u64).cmp(&(a.0 as u64 * a.1 as u64)))
        });
        out
    }

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
            params: self.params(),
            // Something the operating system is telling us about is really
            // there. The number is here for finders that guess; this one does
            // not.
            confidence: 1.0,
        }
    }
}

impl Found {
    /// What `source.add` needs to open this device, and `sizes`, which is for
    /// the form that offers them and is not a setting: a client takes it out
    /// before it adds the source.
    fn params(&self) -> serde_json::Value {
        let mut params = serde_json::json!({ "device": self.id, "label": self.name });
        let sizes: Vec<String> = self.sizes().iter().map(|(w, h)| format!("{w}x{h}")).collect();
        if !sizes.is_empty() {
            params["sizes"] = serde_json::json!(sizes);
        }
        params
    }
}

/// The size to ask a device for when nobody chose one.
///
/// A capture element asked for "anything" settles on the first mode its device
/// lists, whatever order the request was written in, and a MacBook Pro camera
/// lists 1080x1920 first: a picture that Photo Booth shows wide arrived as a
/// tall strip in the middle of a wide canvas. So one size is chosen here and
/// asked for by name. The shape of the canvas first, because a picture of
/// another shape is letterboxed; then the smallest that is at least the
/// canvas, because nothing is gained by scaling down from more; then the
/// largest there is. A device whose every mode is the wrong shape gets its
/// widest, which is still the right way up.
pub fn pick_size(sizes: &[(u32, u32)], canvas: (u32, u32)) -> Option<(u32, u32)> {
    let shape = |s: &(u32, u32)| s.0 as f64 / s.1.max(1) as f64;
    let wanted = shape(&canvas);
    let area = |s: &(u32, u32)| s.0 as u64 * s.1 as u64;
    let best_of = |pool: Vec<(u32, u32)>| -> Option<(u32, u32)> {
        let enough: Vec<_> = pool.iter().copied().filter(|s| s.0 >= canvas.0 && s.1 >= canvas.1).collect();
        enough.iter().copied().min_by_key(area).or_else(|| pool.iter().copied().max_by_key(area))
    };
    let same_shape: Vec<_> = sizes.iter().copied().filter(|s| (shape(s) - wanted).abs() < 0.02).collect();
    if !same_shape.is_empty() {
        return best_of(same_shape);
    }
    let upright = wanted >= 1.0;
    let same_way: Vec<_> = sizes.iter().copied().filter(|s| (s.0 >= s.1) == upright).collect();
    if !same_way.is_empty() {
        return best_of(same_way);
    }
    best_of(sizes.to_vec())
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
        "no device matches '{wanted}'. This machine has: {}. Choose one of those, or leave \
         the device empty for the first one. A name for the source goes in its label.",
        devices
            .iter()
            .map(|d| format!("'{}' ({})", d.name, d.id))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Whether the platform's monitor lists any device under these classes.
///
/// What a plugin asks before it falls back from [`find`] to a capture element
/// by name. The fallback is for a machine whose monitor is missing or sees
/// nothing. When the monitor does list devices and the one asked for is not
/// among them, `find` has already said so and named the ones there are, and
/// trying the bare element instead buried that under a complaint about an
/// `element` setting nobody had touched: somebody typed a name into the
/// camera box and was told to clear a field that was empty.
pub fn lists_any(classes: &[&str]) -> bool {
    list(classes).map(|found| !found.is_empty()).unwrap_or(false)
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

    /// The modes of the MacBook Pro camera this was found on, in its own order.
    const MACBOOK: &[(u32, u32)] =
        &[(1080, 1920), (1920, 1080), (1328, 1760), (1760, 1328), (1552, 1552), (1280, 720), (640, 480)];

    #[test]
    fn a_camera_that_lists_a_tall_mode_first_is_still_opened_wide() {
        assert_eq!(pick_size(MACBOOK, (1920, 1080)), Some((1920, 1080)));
        assert_eq!(pick_size(MACBOOK, (1280, 720)), Some((1280, 720)));
        // A canvas larger than anything on offer takes the largest of its shape.
        assert_eq!(pick_size(MACBOOK, (3840, 2160)), Some((1920, 1080)));
        // An upright canvas is a choice somebody made, and gets an upright picture.
        assert_eq!(pick_size(MACBOOK, (1080, 1920)), Some((1080, 1920)));
        // No mode of the canvas's shape: the right way up, and enough of it.
        assert_eq!(pick_size(&[(1080, 1920), (1600, 1200), (640, 480)], (1280, 720)), Some((1600, 1200)));
        assert_eq!(pick_size(&[], (1920, 1080)), None);
    }

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
