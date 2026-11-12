//! `schemas/ptz.json`, read into structs. Anything missing or wrong falls
//! back to its default and is named in `problems`, so a typo in one camera
//! never stops the others from working.

use std::path::PathBuf;
use std::time::Duration;

mod camera;

pub use camera::{check_slug, CameraSpec};

use godwinmix_track::Framing;
use serde_json::Value;

use crate::follow::detector::Kind;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub cameras: Vec<CameraSpec>,
    /// The framing a follow starts with.
    pub framing: Framing,
    /// How wide the follow loop asks for the picture, in pixels.
    pub track_width: u32,
    /// Where presets' thumbnails are kept.
    pub data_dir: PathBuf,
    /// How long the joystick keeps a camera moving after the last call.
    pub joystick_hold: Duration,
    /// The follow loop's person detector: motion, yolox or yolox+pose.
    pub detector: Kind,
    /// Where the detector's models are kept; the tracking engine's
    /// `models_dir()` when not set.
    pub models_dir: Option<PathBuf>,
    pub problems: Vec<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            cameras: Vec::new(),
            framing: Framing::Waist,
            track_width: 320,
            data_dir: default_data_dir(),
            joystick_hold: Duration::from_millis(300),
            detector: Kind::Yolox,
            models_dir: None,
            problems: Vec::new(),
        }
    }
}

/// `$GODWINMIX_HOME/ptz`, or `.godwinmix/ptz` in the home directory.
pub fn default_data_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("GODWINMIX_HOME") {
        return PathBuf::from(home).join("ptz");
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join(".godwinmix").join("ptz")
}

impl Settings {
    pub fn from_value(v: &Value) -> Settings {
        let mut s = Settings::default();
        for c in v.get("cameras").and_then(Value::as_array).into_iter().flatten() {
            match CameraSpec::from_value(c) {
                Ok(spec) if s.cameras.iter().any(|k| k.id == spec.id) => s.problems.push(format!("camera '{}' is listed twice; the first is kept", spec.id)),
                Ok(spec) => s.cameras.push(spec),
                Err(why) => s.problems.push(why),
            }
        }
        if let Some(f) = v.get("framing").and_then(Value::as_str) {
            match Framing::from_slug(f) {
                Some(f) => s.framing = f,
                None => s.problems.push(format!("framing '{f}' is not one of close, waist, full, wide; waist is used")),
            }
        }
        if let Some(w) = v.get("track_width").and_then(Value::as_u64) {
            s.track_width = w.clamp(160, 640) as u32;
        }
        if let Some(d) = v.get("data_dir").and_then(Value::as_str).filter(|d| !d.is_empty()) {
            s.data_dir = PathBuf::from(d);
        }
        if let Some(ms) = v.get("joystick_hold_ms").and_then(Value::as_u64) {
            s.joystick_hold = Duration::from_millis(ms.clamp(100, 2_000));
        }
        if let Some(d) = v.get("detector").and_then(Value::as_str) {
            match Kind::parse(d) {
                Some(k) => s.detector = k,
                None => s.problems.push(format!("detector '{d}' is not one of motion, yolox, yolox+pose; yolox is used")),
            }
        }
        s.models_dir = v.get("models_dir").and_then(Value::as_str).filter(|d| !d.is_empty()).map(PathBuf::from);
        s
    }
}
