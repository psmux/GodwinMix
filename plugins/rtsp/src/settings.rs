//! `rtsp/output` params: which port, which path, which address to listen on.

use serde_json::Value;

/// The port RTSP servers use when nobody says otherwise. 554 needs root on
/// Linux and macOS, so the common alternative is the default.
pub const DEFAULT_PORT: u16 = 8554;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub port: u16,
    /// The mount path, with its leading slash: `/live`.
    pub path: String,
    /// The address to listen on. `0.0.0.0` is every interface.
    pub bind: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { port: DEFAULT_PORT, path: "/live".into(), bind: "0.0.0.0".into() }
    }
}

impl Settings {
    pub fn from_params(params: &Value) -> Result<Settings, String> {
        let mut s = Settings::default();
        if let Some(v) = params.get("port").filter(|v| !v.is_null()) {
            let port = v.as_u64().filter(|p| (1..=65535).contains(p)).ok_or_else(|| {
                format!("rtsp/output port must be a whole number from 1 to 65535, got {v}. 8554 is the usual choice; 554 needs the machine's administrator.")
            })?;
            s.port = port as u16;
        }
        if let Some(v) = params.get("path").and_then(Value::as_str).filter(|p| !p.trim().is_empty()) {
            s.path = mount(v)?;
        }
        if let Some(v) = params.get("bind").and_then(Value::as_str).filter(|b| !b.trim().is_empty()) {
            let ok = v.parse::<std::net::IpAddr>().is_ok();
            if !ok {
                return Err(format!("rtsp/output bind must be an IP address such as 0.0.0.0 or 192.168.1.10, got {v}"));
            }
            s.bind = v.to_string();
        }
        Ok(s)
    }

    /// The address a player opens, with `host` for the machine's name.
    pub fn url(&self, host: &str) -> String {
        format!("rtsp://{host}:{}{}", self.port, self.path)
    }

    pub fn describe(&self) -> String {
        let host = if self.bind == "0.0.0.0" { "<this machine>" } else { &self.bind };
        self.url(host)
    }
}

/// `live`, `/live` and `/studio/main` are paths; spaces and `..` are not.
fn mount(path: &str) -> Result<String, String> {
    let p = format!("/{}", path.trim().trim_start_matches('/'));
    let fine = p.len() > 1
        && !p.contains("..")
        && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'));
    if !fine {
        return Err(format!(
            "rtsp/output path {path:?} is not a usable address. Use letters, digits, - and _ with / between parts, such as live or studio/main."
        ));
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_then_each_field() {
        assert_eq!(Settings::from_params(&json!({})).unwrap(), Settings::default());
        let s = Settings::from_params(&json!({"port": 9554, "path": "studio/main", "bind": "127.0.0.1"})).unwrap();
        assert_eq!(s.url("127.0.0.1"), "rtsp://127.0.0.1:9554/studio/main");
        assert_eq!(s.describe(), "rtsp://127.0.0.1:9554/studio/main");
        assert_eq!(Settings::default().describe(), "rtsp://<this machine>:8554/live");
    }

    #[test]
    fn a_bad_value_says_what_would_do() {
        assert!(Settings::from_params(&json!({"port": 0})).unwrap_err().contains("8554"));
        assert!(Settings::from_params(&json!({"path": "../etc"})).unwrap_err().contains("studio/main"));
        assert!(Settings::from_params(&json!({"path": "a b"})).is_err());
        assert!(Settings::from_params(&json!({"bind": "everywhere"})).unwrap_err().contains("0.0.0.0"));
    }
}
