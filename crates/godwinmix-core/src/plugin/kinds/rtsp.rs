//! What an RTSP address needs that `uridecodebin` does not give it.
//!
//! `uridecodebin` opens every RTSP scheme with `rtspsrc`, and `rtspsrc` ships
//! with numbers chosen for a player on a desk: a two second jitter buffer,
//! twenty seconds before a silent TCP connection counts as dead. A mixer wants
//! a camera that comes up quickly and fails quickly, so the supervisor can
//! bring it back. This sets those numbers on the `rtspsrc` the bin makes,
//! from `source-setup`, before it opens anything.
//!
//! The schemes are `rtspsrc`'s own. `rtsp://` tries UDP and falls back to TCP
//! when nothing arrives over UDP within `timeout`; `rtspt://` is TCP only,
//! `rtspu://` UDP only and `rtsph://` RTSP tunnelled over HTTP. `params.transport`
//! says the same thing for a plain `rtsp://` address.

use crate::config::Params;
use anyhow::{ensure, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::debug;

/// Every scheme `rtspsrc` answers to. All of them are continuous streams.
pub const SCHEMES: [&str; 9] = [
    "rtsp://", "rtspt://", "rtspu://", "rtsph://", "rtsps://", "rtspst://", "rtspsu://", "rtspsh://",
    "rtsp-sdp://",
];

/// The jitter buffer, in milliseconds, when `params.latency_ms` is left out.
/// Enough for a camera on a LAN or a decent uplink; `rtspsrc`'s own 2000 put
/// two seconds between the camera and the programme for nothing.
pub const LATENCY_MS: u32 = 200;
/// How long a connection may stay silent before the source gives up on it
/// and says so. Also how long `rtsp://` waits for UDP before trying TCP.
pub const SILENCE_US: u64 = 5_000_000;
/// How long a stop waits for the camera to answer TEARDOWN. A camera behind a
/// pulled cable never answers, and the stop must not wait on it.
pub const TEARDOWN_US: u64 = 200_000;

const TRANSPORTS: [&str; 3] = ["auto", "tcp", "udp"];

/// Whether `uri` is one `rtspsrc` opens.
pub fn is_rtsp(uri: &str) -> bool {
    let lower = uri.trim().to_ascii_lowercase();
    SCHEMES.iter().any(|s| lower.starts_with(s))
}

/// Whether the scheme itself already names a transport, which then wins over
/// `params.transport`: `rtspt://` with `transport = "udp"` is still TCP.
fn scheme_names_transport(uri: &str) -> bool {
    let lower = uri.trim().to_ascii_lowercase();
    !(lower.starts_with("rtsp://") || lower.starts_with("rtsps://"))
}

/// Refuse what `rtspsrc` would refuse later and less clearly.
pub fn validate(params: &Params) -> Result<()> {
    if let Some(v) = params.get("transport") {
        ensure!(
            v.as_str().is_some_and(|s| TRANSPORTS.contains(&s.to_ascii_lowercase().as_str())),
            "params.transport must be \"auto\", \"tcp\" or \"udp\", not {v}. Choose tcp for a camera \
             across a firewall, a VPN or NAT; auto tries UDP and falls back to TCP"
        );
    }
    if let Some(v) = params.get("latency_ms") {
        ensure!(
            v.as_integer().is_some_and(|n| (0..=10_000).contains(&n)),
            "params.latency_ms must be a whole number of milliseconds from 0 to 10000, not {v}. \
             Raise it from {LATENCY_MS} for a camera on a jittery network"
        );
    }
    Ok(())
}

/// What `rtspsrc` is told for one source.
#[derive(Debug, Clone, PartialEq)]
pub struct Tuning {
    pub latency_ms: u32,
    /// The `protocols` flags, or `None` to leave them to the scheme.
    pub protocols: Option<&'static str>,
}

impl Tuning {
    pub fn of(uri: &str, params: &Params) -> Tuning {
        let latency_ms = params
            .get("latency_ms")
            .and_then(|v| v.as_integer())
            .map_or(LATENCY_MS, |n| n.clamp(0, 10_000) as u32);
        let asked = params.get("transport").and_then(|v| v.as_str()).unwrap_or("auto").to_ascii_lowercase();
        let protocols = match (scheme_names_transport(uri), asked.as_str()) {
            (true, _) => None,
            (false, "tcp") => Some("tcp"),
            (false, "udp") => Some("udp"),
            // UDP first, then TCP: what a camera on the same LAN wants, and
            // what still works when a firewall drops the UDP.
            (false, _) => Some("udp-mcast+udp+tcp"),
        };
        Tuning { latency_ms, protocols }
    }

    /// Set this on an `rtspsrc`.
    pub fn apply(&self, src: &gst::Element) {
        set(src, "latency", self.latency_ms);
        if let Some(p) = self.protocols {
            if src.find_property("protocols").is_some() {
                src.set_property_from_str("protocols", p);
            }
        }
        set(src, "timeout", SILENCE_US);
        set(src, "tcp-timeout", SILENCE_US);
        set(src, "teardown-timeout", TEARDOWN_US);
        set(src, "do-rtsp-keep-alive", true);
        // A frame later than the jitter buffer is dropped rather than queued
        // behind, so a burst after a stall cannot push the picture back.
        set(src, "drop-on-latency", true);
    }
}

fn set<V: Into<gst::glib::Value>>(el: &gst::Element, name: &str, v: V) {
    if el.find_property(name).is_some() {
        el.set_property(name, v);
    }
}

/// Tune the `rtspsrc` that `decode`, a `uridecodebin`, makes for `uri`.
pub fn tune(decode: &gst::Element, uri: &str, params: &Params) {
    let tuning = Tuning::of(uri, params);
    decode.connect("source-setup", false, move |args| {
        let src = args.get(1).and_then(|v| v.get::<gst::Element>().ok())?;
        if src.factory().is_some_and(|f| f.name() == "rtspsrc") {
            debug!(?tuning, "rtspsrc tuned for a live source");
            tuning.apply(&src);
        }
        None
    });
}

#[cfg(test)]
#[path = "rtsp_tests.rs"]
mod tests;
