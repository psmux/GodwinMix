//! `decklink/source` params: which card input, which connector, which video
//! mode, and whether to take its embedded sound.

use serde_json::Value;

/// The connectors `decklinkvideosrc` knows, by the names it takes.
pub const CONNECTIONS: &[&str] = &["auto", "sdi", "hdmi", "optical-sdi", "component", "composite", "svideo"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The input's index on this machine: 0 for the first input of the first card.
    pub device_number: i32,
    pub connection: String,
    /// `auto` detects the signal; otherwise a `decklinkvideosrc` mode nick
    /// such as `1080p25` or `1080i50`.
    pub mode: String,
    /// Take the sound embedded in the SDI or HDMI signal.
    pub audio: bool,
    pub label: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { device_number: 0, connection: "auto".into(), mode: "auto".into(), audio: true, label: String::new() }
    }
}

impl Settings {
    pub fn from_params(p: &Value) -> Result<Settings, String> {
        let mut s = Settings::default();
        if let Some(v) = p.get("device_number").filter(|v| !v.is_null()) {
            s.device_number = v.as_i64().filter(|n| (0..64).contains(n)).ok_or("decklink/source device_number must be 0 to 63: 0 is the first input of the first card")? as i32;
        }
        if let Some(c) = p.get("connection").and_then(Value::as_str).filter(|c| !c.is_empty()) {
            if !CONNECTIONS.contains(&c) {
                return Err(format!("decklink/source connection must be one of {}, not {c}", CONNECTIONS.join(", ")));
            }
            s.connection = c.to_string();
        }
        if let Some(m) = p.get("mode").and_then(Value::as_str).filter(|m| !m.is_empty()) {
            let fine = m.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
            if !fine {
                return Err(format!("decklink/source mode {m:?} is not a video mode. Use auto, or one such as 1080p25 or 1080i50."));
            }
            s.mode = m.to_string();
        }
        s.audio = p.get("audio").and_then(Value::as_bool).unwrap_or(true);
        s.label = p.get("label").and_then(Value::as_str).unwrap_or("").to_string();
        Ok(s)
    }

    pub fn describe(&self) -> String {
        if self.label.is_empty() {
            format!("DeckLink input {}", self.device_number)
        } else {
            format!("'{}'", self.label)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_and_each_field() {
        assert_eq!(Settings::from_params(&json!({})).unwrap(), Settings::default());
        let s = Settings::from_params(&json!({"device_number": 2, "connection": "sdi", "mode": "1080i50", "audio": false, "label": "Cam 3"})).unwrap();
        assert_eq!((s.device_number, s.connection.as_str(), s.mode.as_str(), s.audio), (2, "sdi", "1080i50", false));
        assert_eq!(s.describe(), "'Cam 3'");
    }

    #[test]
    fn a_bad_value_says_what_would_do() {
        assert!(Settings::from_params(&json!({"connection": "vga"})).unwrap_err().contains("sdi"));
        assert!(Settings::from_params(&json!({"device_number": -1})).unwrap_err().contains("first input"));
        assert!(Settings::from_params(&json!({"mode": "10 80"})).unwrap_err().contains("1080p25"));
    }
}
