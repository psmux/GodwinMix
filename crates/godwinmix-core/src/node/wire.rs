//! What crosses the node bridge.
//!
//! The same JSON-RPC 2.0 the core speaks to every other client, on a WebSocket,
//! with one addition: a frame that belongs to a plugin instance carries
//! `instance` in its params, so one socket multiplexes every plugin the node
//! hosts. That is the whole difference between a sidecar's stdin and a node's
//! socket, and it is why `configure`, `health`, `seek` and `tool.call` need no
//! second implementation on the far side.

use godwinmix_protocol::plugin::manifest::Manifest as PluginManifest;
use godwinmix_protocol::plugin::wire::Canvas;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The bridge's own version. Bumped when a frame changes shape, checked at
/// hello, and reported by `node.get` so an operator can see a node that is
/// behind before it misbehaves.
pub const BRIDGE_API: u32 = 1;

/// How often a node sends a heartbeat.
pub const HEARTBEAT_EVERY_MS: u64 = 1_000;

/// How long the core waits before it calls a node gone. Three missed beats,
/// which is 04 section 5.
pub const HEARTBEAT_TOLERANCE_MS: u64 = 3_000;

/// How long a call over the bridge may take. The same five seconds every
/// method is held to, so a node that has gone quiet fails a call rather than
/// holding a mixer command open.
pub const CALL_TIMEOUT_MS: u64 = 5_000;

/// One JSON-RPC frame, in either direction.
///
/// Deliberately not the plugin crate's `Frame`: that one is built for lines on
/// a pipe with a 4 MiB cap and a non-JSON escape hatch for a Python traceback.
/// A WebSocket frame is already framed and already UTF-8.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<FrameError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Frame {
    pub fn request(id: i64, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    pub fn notification(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: None,
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    pub fn answer(id: i64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            method: None,
            params: None,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(id: i64, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            method: None,
            params: None,
            result: None,
            error: Some(FrameError { code, message: message.into(), data: None }),
        }
    }
}

/// The first frame a node sends after the TLS handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    /// The name the operator gave this node. It must match the name in the
    /// certificate, or the core refuses the connection.
    pub name: String,
    pub version: String,
    pub api: u32,
    /// `linux-x86_64` and friends, so `node.get` can say why a plugin will not
    /// run there.
    pub platform: String,
    /// Every plugin this node has, whole manifests, so the core's picker can
    /// offer them without the plugin being installed on the core at all.
    #[serde(default)]
    pub plugins: Vec<PluginManifest>,
    /// The address the core should send media to, host only. The port comes
    /// per instance.
    #[serde(default)]
    pub media_host: String,
    /// The settings schema for each provide that has one, keyed
    /// `<plugin>/<provide>`.
    ///
    /// Sent with the manifests rather than fetched on demand: a settings form
    /// is what an operator opens first, and a read method that waited on a
    /// network round trip to render one would be the first thing to feel
    /// remote. A schema is a few kilobytes and a node has a handful of
    /// plugins.
    #[serde(default)]
    pub schemas: std::collections::BTreeMap<String, Value>,
}

/// What the core answers a hello with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Welcome {
    pub core: String,
    pub version: String,
    pub api: u32,
    pub canvas: Canvas,
    /// Where the node's `GstNetClientClock` should point.
    pub clock: ClockOffer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockOffer {
    pub host: String,
    pub port: i32,
    /// `net` or `ptp`. PTP is the config switch of 04 section 4.
    pub kind: String,
}

/// One beat, every second.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Heartbeat {
    pub ts_unix_ms: u64,
    /// How far the node's clock sits from the core's, in milliseconds.
    pub clock_offset_ms: f64,
    /// The spread of the last few offset readings.
    pub clock_jitter_ms: f64,
    /// True once the client clock has synced. The node starts no plugin
    /// before it has.
    pub clock_synced: bool,
    pub instances: Vec<InstanceReport>,
}

/// What the node says about one instance it hosts. This is the "actual state"
/// half of the reconciler in 04 section 6.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceReport {
    pub instance: String,
    /// The lifecycle state as `InstanceState` spells it.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub latency_ms: u32,
}

/// `node.spawn`: start one plugin instance on the node and point its media at
/// the core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spawn {
    pub instance: String,
    /// `<plugin>/<provide>`, the same string `type` takes in the config.
    pub type_id: String,
    #[serde(default)]
    pub params: Value,
    pub canvas: Canvas,
    /// How the media gets to the core.
    pub media: MediaPlan,
}

/// `node.stop`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stop {
    pub instance: String,
    #[serde(default)]
    pub reason: String,
}

/// A frame addressed to one instance: `configure`, `health`, `seek`,
/// `position`, `keyframe`, `audio.set`, `tool.call`, `render`, `discover`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToInstance {
    pub instance: String,
    #[serde(flatten)]
    pub rest: Value,
}

/// How media crosses between a node and the core.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum BridgeTransport {
    /// RTP over UDP with RFC 6051 header extensions. The default: the receiver
    /// knows the sender's timeline from the first packet.
    #[default]
    Rtp,
    /// SRT, for a link that loses packets. ARQ without inventing anything.
    Srt,
    /// WHIP, for a WAN or a NAT nobody controls.
    Whip,
}

impl BridgeTransport {
    pub const fn as_str(self) -> &'static str {
        match self {
            BridgeTransport::Rtp => "rtp",
            BridgeTransport::Srt => "srt",
            BridgeTransport::Whip => "whip",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rtp" => Some(BridgeTransport::Rtp),
            "srt" => Some(BridgeTransport::Srt),
            "whip" => Some(BridgeTransport::Whip),
            _ => None,
        }
    }

    /// The default latency budget in milliseconds. 04 section 4: 200 for RTP,
    /// 120 for SRT plus the encoder's delay.
    pub const fn default_latency_ms(self) -> u32 {
        match self {
            BridgeTransport::Rtp => 200,
            BridgeTransport::Srt => 120,
            BridgeTransport::Whip => 250,
        }
    }

    pub const ALL: [BridgeTransport; 3] =
        [BridgeTransport::Rtp, BridgeTransport::Srt, BridgeTransport::Whip];
}

/// Where one instance's media goes and how.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaPlan {
    pub transport: BridgeTransport,
    /// The address the node sends to, or listens on, depending on the
    /// transport. `host:port` for RTP, a full `srt://` or `https://` URI
    /// otherwise.
    pub target: String,
    /// The second UDP port, for RTP's audio stream. RTP video takes `target`'s
    /// port; audio takes this one.
    #[serde(default)]
    pub audio_port: u16,
    /// The budget the core answers the LATENCY query with. Declared at
    /// ingress, which is what keeps two remote cameras in lip sync.
    pub latency_ms: u32,
}

/// What `node.get` reports about one node, and what `node.list` reports about
/// all of them.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeView {
    pub name: String,
    /// `online`, `offline`, or `expected` for a node listed in the config that
    /// has never dialled in.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Milliseconds since the last heartbeat. The same number
    /// `gmx_node_heartbeat_age_ms` carries.
    pub heartbeat_age_ms: u64,
    pub clock_offset_ms: f64,
    pub clock_jitter_ms: f64,
    pub clock_synced: bool,
    /// The provide ids this node can run, `<plugin>/<provide>`.
    #[serde(default)]
    pub provides: Vec<String>,
    /// The plugins this node has, name and version.
    #[serde(default)]
    pub plugins: Vec<NodePlugin>,
    /// The instances it is hosting right now.
    #[serde(default)]
    pub instances: Vec<NodeInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodePlugin {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub provides: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeInstance {
    pub instance: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub latency_ms: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_round_trips() {
        let f = Frame::request(7, "node.spawn", serde_json::json!({"instance": "cam1"}));
        let text = serde_json::to_string(&f).unwrap();
        let back: Frame = serde_json::from_str(&text).unwrap();
        assert_eq!(back.id, Some(7));
        assert_eq!(back.method.as_deref(), Some("node.spawn"));
        assert!(back.result.is_none(), "a request carries no result field");
    }

    #[test]
    fn every_transport_has_a_budget_and_a_name() {
        for t in BridgeTransport::ALL {
            assert_eq!(BridgeTransport::parse(t.as_str()), Some(t));
            assert!(t.default_latency_ms() >= 120);
        }
    }
}
