//! `schemas/ptz.json`, read into structs. Anything missing or wrong falls
//! back to its default and is named in `problems`, so a typo in one camera
//! never stops the others from working.

use std::path::PathBuf;
use std::time::Duration;

use godwinmix_ptz::{Protocol, Target};
use godwinmix_track::Framing;
use serde_json::{json, Value};

/// One camera as a person describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraSpec {
    /// A slug, `cam-pulpit`.
    pub id: String,
    pub protocol: Protocol,
    pub address: String,
    pub user: String,
    pub password: String,
    pub profile: Option<String>,
    /// The mixer source that shows this camera's picture, for presets'
    /// thumbnails and for tracking.
    pub source: Option<String>,
    /// A frame bus name to read the picture from instead, `camera:cam-pulpit`.
    /// Linux and macOS only.
    pub bus: Option<String>,
}

impl CameraSpec {
    pub fn target(&self) -> Target {
        let mut t = Target::new(self.protocol, &self.address);
        (t.user, t.password, t.profile) = (self.user.clone(), self.password.clone(), self.profile.clone());
        t
    }

    /// The record a caller sees: everything but the password.
    pub fn public(&self) -> Value {
        json!({
            "id": self.id, "protocol": self.protocol.slug(), "address": self.address,
            "user": self.user, "profile": self.profile, "source": self.source, "bus": self.bus,
        })
    }

    pub fn from_value(v: &Value) -> Result<CameraSpec, String> {
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let id = text("id").ok_or("a camera needs an `id`, a slug such as cam-pulpit")?;
        check_slug(&id)?;
        let protocol = Protocol::parse(&text("protocol").unwrap_or_else(|| "visca-ip".into())).map_err(|e| e.message)?;
        let address = text("address").unwrap_or_default();
        if address.is_empty() && protocol != Protocol::Fake {
            return Err(format!("camera '{id}' needs an `address`, the camera's IP address such as 192.0.2.20"));
        }
        Ok(CameraSpec {
            id,
            protocol,
            address,
            user: text("user").unwrap_or_default(),
            password: text("password").unwrap_or_default(),
            profile: text("profile"),
            source: text("source"),
            bus: text("bus"),
        })
    }
}

/// Lower case letters, digits and single hyphens, starting with a letter.
pub fn check_slug(id: &str) -> Result<(), String> {
    let ok = id.len() <= 64
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.ends_with('-')
        && !id.contains("--");
    if ok {
        Ok(())
    } else {
        Err(format!("'{id}' is not a slug. Use lower case letters, digits and single hyphens, starting with a letter: cam-pulpit."))
    }
}

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
    pub problems: Vec<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { cameras: Vec::new(), framing: Framing::Waist, track_width: 320, data_dir: default_data_dir(), joystick_hold: Duration::from_millis(300), problems: Vec::new() }
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
        s
    }
}
