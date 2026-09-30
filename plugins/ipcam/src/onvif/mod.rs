//! ONVIF: find the cameras on the LAN and the RTSP address of each of their
//! profiles, so a person picks a camera from a list instead of typing an
//! address from its manual.
//!
//! * [`wsd`]: the WS-Discovery probe.
//! * [`http`]: a SOAP POST with a deadline.
//! * [`soap`]: GetCapabilities, GetProfiles, GetStreamUri, with a
//!   WS-Security login.

pub mod http;
pub mod soap;
pub mod wsd;

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use soap::Login;

/// One profile of one camera, ready to add as a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    pub name: String,
    /// The RTSP address, with the login in it when one was used.
    pub uri: String,
}

/// What a discovery found: the streams, and the cameras that would say
/// nothing without a login.
#[derive(Debug, Default)]
pub struct Found {
    pub streams: Vec<Stream>,
    pub locked: Vec<String>,
}

/// Probe `to` (the ONVIF multicast group, or one device) and resolve every
/// answer within `wait` altogether.
pub fn discover(to: SocketAddr, wait: Duration, login: &Login) -> Found {
    let until = Instant::now() + wait;
    let devices = wsd::probe(to, wait / 2).unwrap_or_default();
    let mut found = Found::default();
    for d in devices {
        let Some(xaddr) = d.xaddrs.iter().find(|x| x.starts_with("http://") && !x.contains('[')).or(d.xaddrs.first()) else { continue };
        let left = until.saturating_duration_since(Instant::now()).max(Duration::from_millis(300));
        let label = if d.name.is_empty() { xaddr.clone() } else { d.name.clone() };
        match streams(xaddr, login, left) {
            Ok(list) => found.streams.extend(list.into_iter().map(|(profile, uri)| Stream { name: format!("{label} ({profile})"), uri })),
            Err(http::Failure::Unauthorized) => found.locked.push(label),
            Err(http::Failure::Other(_)) => {}
        }
    }
    found
}

fn streams(device: &str, login: &Login, timeout: Duration) -> Result<Vec<(String, String)>, http::Failure> {
    let media = soap::media_service(device, login, timeout)?;
    let mut out = Vec::new();
    for (token, name) in soap::profiles(&media, login, timeout)? {
        let uri = soap::stream_uri(&media, login, &token, timeout)?;
        out.push((name, with_login(&uri, login)));
    }
    Ok(out)
}

/// `rtsp://host/…` with the login put in, for the core to open.
pub fn with_login(uri: &str, login: &Login) -> String {
    if login.user.is_empty() || uri.contains('@') {
        return uri.to_string();
    }
    match uri.split_once("://") {
        Some((scheme, rest)) => format!("{scheme}://{}:{}@{rest}", encode(&login.user), encode(&login.password)),
        None => uri.to_string(),
    }
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
pub mod tests;
#[cfg(test)]
pub use tests::{camera as tests_camera, PASSWORD as TEST_PASSWORD};
