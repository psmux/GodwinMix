//! Nodes: the same plugin, on another machine.
//!
//! A plugin author writes one program. The operator decides where it runs by
//! writing `place` in the config: `core`, `in-process`, `sidecar`, or
//! `node:studio-b`. Nothing else changes. The settings form, the tools, the
//! health, the events and the alerts are identical whether the process is
//! beside the core or in the next building, and that is the thing OBS with
//! DistroAV cannot do: there the remote source arrives as a flattened stream
//! and everything about it is invisible locally.
//!
//! ```text
//!  core                                     node studio-b
//!  +-----------------------+                +-----------------------+
//!  | programme clock       |  UDP           | GstNetClientClock     |
//!  | GstNetTimeProvider    | ------------>  | (calibrated, synced)  |
//!  |                       |                |                       |
//!  | node bridge, mTLS     | <=== WSS ====> | one socket, every     |
//!  |   one peer per node   |                | instance multiplexed  |
//!  |                       |                |                       |
//!  | receive, decode,      | <--- RTP ----  | plugin, encode, send  |
//!  | normalise like any    |      or SRT    | (unixfd to the plugin |
//!  | other source          |                |  exactly as the core  |
//!  +-----------------------+                |  would)               |
//!                                           +-----------------------+
//! ```
//!
//! What is in here:
//!
//! * `ca` and `enrol`: the core is its own certificate authority, and the one
//!   time token is the only secret that ever crosses in the clear.
//! * `wire` and `bridge`: one JSON-RPC peer over one WebSocket, symmetric,
//!   with `instance` in the params saying which plugin a frame is for.
//! * `registry`: what the core knows about each node, connected or not.
//! * `clock`: the net time provider, the client clock, PTP as an option.
//! * `media`: RTP, SRT and WHIP between the two machines, and the latency
//!   budget declared at ingress.
//! * `server` and `daemon`: the two ends.
//! * `reconcile`: desired state against what each node reports, the freeze
//!   frame when one vanishes, and the plan for moving a running source.
//! * `discovery`: mDNS/DNS-SD, so a node on a flat network is found rather
//!   than typed.
//!
//! The one rule that outranks everything else here: the programme output never
//! stops. A node that vanishes marks its plugins failed, the compositor holds
//! the freeze frame for 45 seconds as it already does, then shows the slate
//! and raises an alert. Nothing in this module can block the encoder, because
//! nothing in this module runs on a streaming thread.

pub mod bridge;
pub mod ca;
pub mod clock;
pub mod daemon;
pub mod discovery;
pub mod enrol;
pub mod media;
pub mod reconcile;
pub mod registry;
pub mod runtime;
pub mod server;
pub mod wire;

pub use ca::{Issued, NodeCa};
pub use enrol::{Refusal, Ticket, Tickets};
pub use registry::{NodeRecord, Nodes};
pub use wire::{BridgeTransport, MediaPlan, NodeView};

/// Where a plugin instance runs.
///
/// The config's `place` key, and the manifest's `placements` list, in one
/// type. `Node` carries the name so `place = "node:studio-b"` round trips.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Place {
    /// Compiled into the core. Cannot be asked for by a plugin.
    Core,
    /// Compiled in by a custom build behind a cargo feature.
    InProcess,
    /// A separate process on this machine. The default for third parties.
    #[default]
    Sidecar,
    /// A tier 2 plugin hosted by a node on another machine.
    Node(String),
}

impl Place {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "core" => Some(Place::Core),
            "in-process" => Some(Place::InProcess),
            "sidecar" => Some(Place::Sidecar),
            other => other
                .strip_prefix("node:")
                .filter(|n| !n.is_empty())
                .map(|n| Place::Node(n.to_string())),
        }
    }

    /// The string the config writes and the wire carries.
    pub fn as_config(&self) -> String {
        match self {
            Place::Core => "core".into(),
            Place::InProcess => "in-process".into(),
            Place::Sidecar => "sidecar".into(),
            Place::Node(name) => format!("node:{name}"),
        }
    }

    /// The word a manifest's `placements` list uses. `node:studio-b` and
    /// `node:studio-c` are both the `node` placement as far as a plugin is
    /// concerned, which is the whole point: the plugin does not know which
    /// machine it is on.
    pub const fn declared(&self) -> &'static str {
        match self {
            Place::Core => "core",
            Place::InProcess => "in-process",
            Place::Sidecar => "sidecar",
            Place::Node(_) => "node",
        }
    }

    pub fn node(&self) -> Option<&str> {
        match self {
            Place::Node(name) => Some(name),
            _ => None,
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Place::Node(_))
    }
}

impl std::fmt::Display for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_config())
    }
}

impl serde::Serialize for Place {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_config())
    }
}

impl<'de> serde::Deserialize<'de> for Place {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Place::parse(&text).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "`{text}` is not a placement. Write one of: core, in-process, sidecar, or \
                 node:<name> naming a node in [nodes]"
            ))
        })
    }
}

impl schemars::JsonSchema for Place {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Place".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Where an instance runs: core, in-process, sidecar, or node:<name>.",
            "examples": ["sidecar", "node:studio-b"],
        })
    }
}

/// Refuse a placement the plugin did not declare, in the shape error -32005
/// wants: the message names what it did declare, and so does `data`.
///
/// One function so the message is the same whether the refusal comes from
/// `source.add`, `source.set`, the config loader or the reconciler.
pub fn check_placement(type_id: &str, place: &Place, declared: &[String]) -> anyhow::Result<()> {
    if declared.iter().any(|d| d == place.declared()) {
        return Ok(());
    }
    anyhow::bail!(
        "`{type_id}` does not run at `{}`. It declares: {}. Either change `place`, or ask the \
         plugin's author to declare the placement",
        place.as_config(),
        if declared.is_empty() { "nothing".to_string() } else { declared.join(", ") }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_place_round_trips() {
        for text in ["core", "in-process", "sidecar", "node:studio-b"] {
            let place = Place::parse(text).unwrap();
            assert_eq!(place.as_config(), text);
        }
        assert_eq!(Place::parse("node:"), None, "a node placement needs a name");
        assert_eq!(Place::parse("wasm"), None);
    }

    #[test]
    fn a_node_placement_is_the_node_placement_whatever_the_name() {
        assert_eq!(Place::parse("node:a").unwrap().declared(), "node");
        assert_eq!(Place::parse("node:b").unwrap().declared(), "node");
        assert_eq!(Place::parse("node:b").unwrap().node(), Some("b"));
    }

    #[test]
    fn a_refused_placement_names_what_the_plugin_does_declare() {
        let e = check_placement(
            "ndi/source",
            &Place::Node("studio-b".into()),
            &["sidecar".to_string()],
        )
        .unwrap_err();
        let text = e.to_string();
        assert!(text.contains("node:studio-b"), "{text}");
        assert!(text.contains("sidecar"), "the refusal must list what it does declare: {text}");
    }

    #[test]
    fn a_declared_placement_is_allowed() {
        assert!(check_placement(
            "ndi/source",
            &Place::Node("studio-b".into()),
            &["sidecar".into(), "node".into()],
        )
        .is_ok());
    }

    #[test]
    fn place_serialises_as_the_string_the_config_writes() {
        let json = serde_json::to_string(&Place::Node("studio-b".into())).unwrap();
        assert_eq!(json, "\"node:studio-b\"");
        let back: Place = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Place::Node("studio-b".into()));
    }
}
