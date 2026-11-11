//! One camera as a person describes it, and the slug rule for its id.

use godwinmix_ptz::{Protocol, Target};
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
