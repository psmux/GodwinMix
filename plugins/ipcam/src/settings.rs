//! `ipcam/source` params: the camera's address, how to read it, and the
//! account it wants.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `multipart/x-mixed-replace`: one long response, a JPEG per part.
    Mjpeg,
    /// One JPEG per request, asked for `fps` times a second.
    Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub uri: String,
    pub mode: Mode,
    /// Snapshots a second.
    pub fps: u32,
    pub user: String,
    pub password: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { uri: String::new(), mode: Mode::Mjpeg, fps: 5, user: String::new(), password: String::new() }
    }
}

/// What the address says when `mode` is `auto`: a still picture's name is a
/// snapshot, anything else a stream.
pub fn guess(uri: &str) -> Mode {
    let path = uri.to_lowercase();
    let path = path.split(['?', '#']).next().unwrap_or("");
    let still = [".jpg", ".jpeg", "snapshot", "snap.cgi", "image.cgi", "still"].iter().any(|s| path.contains(s));
    if still && !path.contains("mjpg") && !path.contains("mjpeg") && !path.contains("video") {
        Mode::Snapshot
    } else {
        Mode::Mjpeg
    }
}

impl Settings {
    pub fn from_params(params: &Value) -> Result<Settings, String> {
        let text = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
        let uri = text("uri");
        if !uri.is_empty() && !(uri.starts_with("http://") || uri.starts_with("https://")) {
            return Err(format!(
                "ipcam/source uri must be the camera's http:// or https:// address, such as http://192.168.1.64/video.mjpg; got {uri}. An rtsp:// camera is added as a stream address instead."
            ));
        }
        let mode = match text("mode").as_str() {
            "" | "auto" => guess(&uri),
            "mjpeg" => Mode::Mjpeg,
            "snapshot" => Mode::Snapshot,
            other => return Err(format!("ipcam/source mode must be auto, mjpeg or snapshot, not {other}")),
        };
        let fps = match params.get("fps") {
            None | Some(Value::Null) => 5,
            Some(v) => v.as_u64().filter(|n| (1..=30).contains(n)).ok_or("ipcam/source fps must be 1 to 30 snapshots a second")? as u32,
        };
        Ok(Settings { uri, mode, fps, user: text("user"), password: text("password") })
    }

    /// Something to say why this cannot start, before it tries.
    pub fn problem(&self) -> Option<String> {
        self.uri.is_empty().then(|| "ipcam/source needs the camera's address in uri, such as http://192.168.1.64/video.mjpg".to_string())
    }

    /// The address with any account in it hidden, for logs and health.
    pub fn redacted(&self) -> String {
        match self.uri.split_once("://").and_then(|(s, rest)| rest.split_once('@').map(|(_, host)| (s, host))) {
            Some((scheme, host)) => format!("{scheme}://…@{host}"),
            None => self.uri.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_mode_follows_the_address_unless_told() {
        assert_eq!(guess("http://cam/video.mjpg"), Mode::Mjpeg);
        assert_eq!(guess("http://cam/axis-cgi/mjpg/video.cgi"), Mode::Mjpeg);
        assert_eq!(guess("http://cam/snapshot.jpg"), Mode::Snapshot);
        assert_eq!(guess("http://cam/cgi-bin/snapshot.cgi?chn=1"), Mode::Snapshot);
        let s = Settings::from_params(&json!({"uri": "http://cam/snapshot.jpg", "mode": "mjpeg"})).unwrap();
        assert_eq!(s.mode, Mode::Mjpeg);
    }

    #[test]
    fn a_bad_value_says_what_would_do() {
        assert!(Settings::from_params(&json!({"uri": "rtsp://cam/1"})).unwrap_err().contains("stream address"));
        assert!(Settings::from_params(&json!({"uri": "http://c/x", "fps": 0})).unwrap_err().contains("1 to 30"));
        assert!(Settings::default().problem().is_some());
        let s = Settings::from_params(&json!({"uri": "http://admin:pw@10.0.0.5/video.mjpg"})).unwrap();
        assert_eq!(s.redacted(), "http://…@10.0.0.5/video.mjpg");
    }
}
