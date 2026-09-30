//! `icecast/output` params: the server, the mount, the login and what to
//! encode the sound as. `icecast/source` needs only an address.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mp3,
    OggVorbis,
    OggOpus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub host: String,
    pub port: u16,
    /// With its leading slash: `/live.mp3`.
    pub mount: String,
    pub user: String,
    pub password: String,
    pub format: Format,
    pub bitrate_kbps: u32,
    /// What listeners see in their player and in the server's directory.
    pub name: String,
    pub public: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            host: String::new(),
            port: 8000,
            mount: "/live.mp3".into(),
            user: "source".into(),
            password: String::new(),
            format: Format::Mp3,
            bitrate_kbps: 128,
            name: "GodwinMix".into(),
            public: false,
        }
    }
}

fn text(params: &Value, k: &str) -> String {
    params.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

impl Settings {
    pub fn from_params(p: &Value) -> Result<Settings, String> {
        let mut s = Settings::default();
        if let Some(uri) = p.get("uri").and_then(Value::as_str).filter(|u| !u.trim().is_empty()) {
            s.apply_uri(uri.trim())?;
        }
        for (key, slot) in [("host", &mut s.host), ("user", &mut s.user), ("password", &mut s.password), ("name", &mut s.name)] {
            let v = text(p, key);
            if !v.is_empty() {
                *slot = v;
            }
        }
        let mount = text(p, "mount");
        if !mount.is_empty() {
            s.mount = format!("/{}", mount.trim_start_matches('/'));
        }
        if let Some(v) = p.get("port").filter(|v| !v.is_null()) {
            s.port = v.as_u64().filter(|n| (1..=65535).contains(n)).ok_or("icecast/output port must be 1 to 65535; Icecast listens on 8000 unless told otherwise")? as u16;
        }
        s.format = match text(p, "format").as_str() {
            "" | "mp3" => Format::Mp3,
            "vorbis" | "ogg" => Format::OggVorbis,
            "opus" => Format::OggOpus,
            other => return Err(format!("icecast/output format must be mp3, vorbis or opus, not {other}")),
        };
        if let Some(v) = p.get("bitrate_kbps").filter(|v| !v.is_null()) {
            s.bitrate_kbps = v.as_u64().filter(|n| (32..=320).contains(n)).ok_or("icecast/output bitrate_kbps must be 32 to 320")? as u32;
        }
        s.public = p.get("public").and_then(Value::as_bool).unwrap_or(false);
        Ok(s)
    }

    /// `icecast://user:password@host:port/mount`, the form a streaming host
    /// hands out.
    fn apply_uri(&mut self, uri: &str) -> Result<(), String> {
        let rest = uri.strip_prefix("icecast://").ok_or_else(|| format!("icecast/output uri must look like icecast://source:password@radio.example.com:8000/live.mp3, not {uri}"))?;
        let (login, hostpart) = rest.rsplit_once('@').map(|(l, h)| (Some(l), h)).unwrap_or((None, rest));
        if let Some((u, pw)) = login.and_then(|l| l.split_once(':')) {
            (self.user, self.password) = (u.to_string(), pw.to_string());
        }
        let (hostport, mount) = hostpart.split_once('/').unwrap_or((hostpart, "live.mp3"));
        self.mount = format!("/{mount}");
        match hostport.rsplit_once(':') {
            Some((h, p)) => (self.host, self.port) = (h.to_string(), p.parse().map_err(|_| format!("bad port in {uri}"))?),
            None => self.host = hostport.to_string(),
        }
        Ok(())
    }

    pub fn problem(&self) -> Option<String> {
        if self.host.is_empty() {
            return Some("icecast/output needs the server's address: host, or uri as icecast://source:password@host:8000/live.mp3".into());
        }
        self.password.is_empty().then(|| "icecast/output needs the server's source password, which the streaming host gives you".into())
    }

    pub fn describe(&self) -> String {
        format!("http://{}:{}{}", self.host, self.port, self.mount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_address_or_fields_give_the_same_settings() {
        let a = Settings::from_params(&json!({"uri": "icecast://source:hackme@radio.example.com:8010/church.mp3"})).unwrap();
        assert_eq!((a.host.as_str(), a.port, a.mount.as_str(), a.password.as_str()), ("radio.example.com", 8010, "/church.mp3", "hackme"));
        let b = Settings::from_params(&json!({"host": "radio.example.com", "port": 8010, "mount": "church.mp3", "password": "hackme"})).unwrap();
        assert_eq!(a, b);
        assert_eq!(b.describe(), "http://radio.example.com:8010/church.mp3");
    }

    #[test]
    fn what_is_missing_is_named() {
        assert!(Settings::default().problem().unwrap().contains("address"));
        let s = Settings::from_params(&json!({"host": "r"})).unwrap();
        assert!(s.problem().unwrap().contains("password"));
        assert!(Settings::from_params(&json!({"format": "aac"})).unwrap_err().contains("mp3, vorbis or opus"));
        assert_eq!(Settings::from_params(&json!({"format": "opus"})).unwrap().format, Format::OggOpus);
    }
}
