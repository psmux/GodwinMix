//! The protocols a channel takes its streams over, the ports they need, and
//! the certificate RTMPS answers with.
//!
//! A channel is one set of keys and many ways in: RTMP on the shared RTMP
//! port, SRT on the shared SRT port by stream id, WHIP on the control port,
//! and RTMPS on a port a person chooses. Each listener is open only while a
//! channel that is switched on uses it (dev/plans/shows-and-renditions.md,
//! Decision 1), and `channel.list` says which are open and for which
//! channels.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A way a publisher reaches a channel. RTMPS is `Rtmps`, set apart
/// because it has a port of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChannelProtocol {
    /// `rtmp://<host>:<rtmp port>/<app>/<stream>?psk=<key>`.
    Rtmp,
    /// `srt://<host>:<srt port>?streamid=<app>/<stream>`, the key as the
    /// passphrase or as `?psk=` in the stream id.
    Srt,
    /// `POST http://<host>:<control port>/whip/<app>/<stream>`, the key as
    /// the bearer token.
    Whip,
}

/// What a channel made before protocols existed takes: RTMP.
pub fn rtmp_only() -> Vec<ChannelProtocol> {
    vec![ChannelProtocol::Rtmp]
}

/// RTMPS for one channel: off, or on at a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Rtmps {
    pub enabled: bool,
    /// The port it listens on while enabled. 443 is offered first.
    pub port: u16,
}

impl Default for Rtmps {
    fn default() -> Rtmps {
        Rtmps { enabled: false, port: 443 }
    }
}

/// Where an encoder is pointed for one protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PublishAddress {
    /// `rtmp`, `rtmps`, `srt` or `whip`.
    pub protocol: String,
    /// The address without the key: `srt://10.0.0.5:9000`.
    pub server: String,
    /// The whole address with `<key>` where the key goes.
    pub example: String,
}

/// One listener a channel needs, and whether it is open now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Listener {
    /// `rtmp`, `rtmps`, `srt`, `whip`, `webrtc` (the media ports WHIP
    /// sessions use) or `relay` (the RTMP port on the loopback alone, for the
    /// mixer's own sources, while no channel has RTMP on).
    pub protocol: String,
    /// `tcp` or `udp`.
    pub transport: String,
    pub port: u16,
    /// The last port of a range, for WebRTC media.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_port: Option<u16>,
    pub open: bool,
    /// The channels it is open for. Empty when nothing needs it.
    pub because: Vec<String>,
    /// Bound to 127.0.0.1 only, so nothing off this machine reaches it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub loopback: bool,
    /// Why it is not open although a channel wants it, and what to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// The certificate RTMPS answers with. The private key never leaves the core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CertificateInfo {
    /// `uploaded`, or `self_signed` for one the mixer made.
    pub source: String,
    /// The names it was made for, for a self signed one.
    #[serde(default)]
    pub names: Vec<String>,
    /// SHA-256 of the certificate, as colon separated hex, to compare with
    /// what an encoder shows.
    pub fingerprint: String,
    /// When it was set, RFC 3339 in UTC.
    pub created: String,
}

/// `channel.certificate.set`: a certificate and its private key, as PEM.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CertificateSetRequest {
    /// The certificate, and any chain after it, as PEM.
    pub cert: String,
    /// Its private key, as PEM.
    pub key: String,
}

/// `channel.certificate.generate`: a self signed certificate.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CertificateGenerateRequest {
    /// Host names and addresses it is for. Defaults to this machine's
    /// address, `localhost` and `127.0.0.1`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
}
