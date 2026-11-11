//! Where the mixer is, and the two pictures this plugin asks it for: one
//! JPEG still for a preset's thumbnail, and the MJPEG stream of a source for
//! the follow loop. Both are plain HTTP GETs on the core's own port; the
//! core makes them only while they are asked for.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use godwinmix_ptz::http::split;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Core {
    /// `http://127.0.0.1:8080`
    pub base: String,
    pub token: Option<String>,
}

impl Core {
    /// From `GMX_RPC` (`ws://127.0.0.1:8080/rpc`) or a `--url`.
    pub fn from_url(url: &str, token: Option<String>) -> Option<Core> {
        let rest = ["ws://", "http://"].iter().find_map(|p| url.strip_prefix(p))?;
        let host = rest.split('/').next().filter(|h| !h.is_empty())?;
        Some(Core { base: format!("http://{host}"), token: token.filter(|t| !t.is_empty()) })
    }

    fn query(&self, width: u32, force: bool) -> String {
        let mut q = format!("?width={width}");
        if force {
            q.push_str("&force=true");
        }
        if let Some(t) = &self.token {
            q.push_str(&format!("&token={}", escape(t)));
        }
        q
    }

    pub fn snapshot_url(&self, source: &str, width: u32) -> String {
        format!("{}/api/v1/snapshot/{}{}", self.base, escape(source), self.query(width, true))
    }

    pub fn mjpeg_url(&self, source: &str, width: u32) -> String {
        format!("{}/mjpeg/{}{}", self.base, escape(source), self.query(width, false))
    }

    /// One still of `source`, `width` pixels across.
    pub fn snapshot(&self, source: &str, width: u32) -> Result<Vec<u8>, String> {
        let url = self.snapshot_url(source, width);
        let mut s = open(&url, Duration::from_secs(3))?;
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).map_err(|e| format!("reading the still of '{source}': {e}"))?;
        let at = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("the mixer gave no HTTP answer for the still")?;
        let head = String::from_utf8_lossy(&raw[..at]).to_string();
        let status = head.split_whitespace().nth(1).unwrap_or("");
        if status != "200" {
            let body = String::from_utf8_lossy(&raw[at + 4..]);
            return Err(format!("the mixer answered {status} for a still of '{source}': {}", body.chars().take(300).collect::<String>()));
        }
        Ok(raw[at + 4..].to_vec())
    }
}

/// Connect and send `GET url`. The answer is read by the caller.
pub fn open(url: &str, timeout: Duration) -> Result<TcpStream, String> {
    let (host, port, path) = split(url).map_err(|f| format!("{f:?}"))?;
    let addr = (host.as_str(), port).to_socket_addrs().ok().and_then(|mut a| a.next()).ok_or_else(|| format!("cannot find the mixer at {host}"))?;
    let mut s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("cannot reach the mixer at {host}:{port}: {e}. Is it running?"))?;
    s.set_read_timeout(Some(timeout)).ok();
    write!(s, "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n").map_err(|e| e.to_string())?;
    Ok(s)
}

fn escape(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_is_found_from_its_rpc_address() {
        let c = Core::from_url("ws://127.0.0.1:8080/rpc", Some("t k".into())).unwrap();
        assert_eq!(c.base, "http://127.0.0.1:8080");
        assert_eq!(c.mjpeg_url("cam 1", 320), "http://127.0.0.1:8080/mjpeg/cam%201?width=320&token=t%20k");
        assert!(c.snapshot_url("cam1", 160).contains("/api/v1/snapshot/cam1?width=160&force=true"));
        assert_eq!(Core::from_url("", None), None);
    }
}
