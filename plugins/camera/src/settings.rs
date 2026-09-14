//! What `schemas/source.json` lets an operator change, and nothing else.
//!
//! The core has already validated the object against the schema by the time it
//! arrives, so nothing here reports a bad value. What it does do is treat a
//! missing key as the default, because `configure` in the conformance harness
//! sends one property at a time.

use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub device: String,
    /// `1920x1080`, or none for whatever the camera prefers.
    pub size: Option<(u32, u32)>,
    /// Frames per second to ask the camera for, or none.
    pub framerate: Option<u32>,
    pub label: String,
    /// A forced capture element, for working around a driver bug.
    pub element: String,
}

impl Settings {
    pub fn from(params: &Value) -> Settings {
        Settings {
            device: text(params, "device"),
            size: parse_size(&text(params, "resolution")),
            framerate: params
                .get("framerate")
                .and_then(Value::as_u64)
                .filter(|n| *n > 0)
                .map(|n| n.min(240) as u32),
            label: text(params, "label"),
            element: text(params, "element"),
        }
    }

    /// Does moving from `self` to `next` need the pipeline rebuilt?
    ///
    /// Only the label can change while the camera runs. Everything else is a
    /// property of the open device.
    pub fn needs_restart(&self, next: &Settings) -> bool {
        self.device != next.device
            || self.size != next.size
            || self.framerate != next.framerate
            || self.element != next.element
    }

    /// The caps to ask the device for.
    ///
    /// Three structures, tried in this order: raw frames in system memory,
    /// then Motion JPEG, then H.264. Raw first because it needs no decoder;
    /// the other two because plenty of cameras will not offer 1080p any other
    /// way. Naming `video/x-raw` without a memory feature is also what keeps a
    /// macOS camera from handing back GL textures that `videoconvert` cannot
    /// read.
    pub fn device_caps(&self) -> String {
        let mut fields = String::new();
        if let Some((w, h)) = self.size {
            fields.push_str(&format!(",width={w},height={h}"));
        }
        if let Some(fps) = self.framerate {
            fields.push_str(&format!(",framerate={fps}/1"));
        }
        format!("video/x-raw{fields};image/jpeg{fields};video/x-h264{fields}")
    }
}

fn text(params: &Value, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_object_is_every_default() {
        let s = Settings::from(&json!({}));
        assert_eq!(s, Settings::default());
        assert_eq!(s.device, "");
        assert!(s.size.is_none());
        assert!(s.framerate.is_none());
    }

    #[test]
    fn a_resolution_is_read_and_a_broken_one_is_ignored() {
        assert_eq!(
            Settings::from(&json!({"resolution": "1920x1080"})).size,
            Some((1920, 1080))
        );
        assert_eq!(Settings::from(&json!({"resolution": ""})).size, None);
        assert_eq!(Settings::from(&json!({"resolution": "big"})).size, None);
    }

    #[test]
    fn zero_frames_per_second_means_let_the_camera_choose() {
        assert_eq!(Settings::from(&json!({"framerate": 0})).framerate, None);
        assert_eq!(
            Settings::from(&json!({"framerate": 30})).framerate,
            Some(30)
        );
        assert_eq!(
            Settings::from(&json!({"framerate": 9000})).framerate,
            Some(240)
        );
    }

    #[test]
    fn a_device_id_keeps_its_shape_but_loses_the_spaces_around_it() {
        assert_eq!(
            Settings::from(&json!({"device": "  /dev/video0 "})).device,
            "/dev/video0"
        );
    }

    #[test]
    fn only_the_label_can_change_without_reopening_the_camera() {
        let a = Settings::from(&json!({"device": "0", "label": "Camera 1"}));
        let b = Settings::from(&json!({"device": "0", "label": "Stage wide"}));
        assert!(!a.needs_restart(&b));
        let c = Settings::from(&json!({"device": "1", "label": "Camera 1"}));
        assert!(a.needs_restart(&c));
        let d = Settings::from(&json!({"device": "0", "resolution": "640x480"}));
        assert!(a.needs_restart(&d));
    }

    #[test]
    fn the_device_caps_offer_raw_before_jpeg_before_h264() {
        let caps = Settings::from(&json!({})).device_caps();
        let raw = caps.find("video/x-raw").expect("raw is offered");
        let jpeg = caps.find("image/jpeg").expect("jpeg is offered");
        let h264 = caps.find("video/x-h264").expect("h264 is offered");
        assert!(raw < jpeg && jpeg < h264, "{caps}");
        assert!(
            !caps.contains("memory:"),
            "GL memory must not be asked for: {caps}"
        );
    }

    #[test]
    fn a_size_and_a_rate_reach_every_structure_of_the_device_caps() {
        let caps =
            Settings::from(&json!({"resolution": "1280x720", "framerate": 30})).device_caps();
        assert_eq!(caps.matches("width=1280").count(), 3, "{caps}");
        assert_eq!(caps.matches("framerate=30/1").count(), 3, "{caps}");
    }
}
