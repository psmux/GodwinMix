//! The plugin seam: what every source, output and filter looks like from the
//! core's side of the boundary.
//!
//! Nothing in here knows about RTMP, browsers or files. It knows that a source
//! produces a pipeline with proxy sinks on it at the canvas caps, that an
//! output consumes the encoded programme, and that a filter is a bin dropped
//! between two elements that both speak the canvas contract. The built in
//! kinds in `kinds/` are implementations of these traits with no privileges the
//! contract does not give a third party.
//!
//! The isolation tier is a placement of the same code, not a different
//! interface: `Tier::Core` is what ships, `Tier::InProcess` is a crate compiled
//! in by a custom build, and `Tier::Sidecar` will be one more implementation of
//! `Source` that spawns a process and wires its pipe into the same `MediaEnds`.
//! The core never learns which one it is talking to.

pub mod branch;
pub mod filter;
pub mod filters;
pub mod harness;
pub mod host;
pub mod supervisor;
pub mod loader;
pub mod kinds;
pub mod output;
pub mod outputs;
pub mod remote;
pub mod source;

use crate::state::{SourceHealth, SourceId};
use gstreamer as gst;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub use branch::ProgrammeBranch;
pub use filter::{Filter, FilterSide, FilterSlot, FilterSpec, Insertion};
pub use output::Output;
pub use source::Source;

/// The protocol level this core was written against. A plugin declaring a
/// higher `api` than this is refused; one declaring lower than
/// `API_COMPATIBLE` is too old to load.
pub const API_LEVEL: u32 = 1;
pub const API_COMPATIBLE: u32 = 1;

/// What a plugin provides. The enum from 03 section 2, for the kinds the core
/// hosts: the media kinds it builds pipelines for, and the three the
/// supervisor runs as singletons beside them. The surface kinds (`panel`,
/// `surface`, `preset`, `graphic`, `collection`) are contributions to a
/// client, never a process the core starts, and are `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvideKind {
    Source,
    Output,
    Filter,
    /// Control plane only. One instance per plugin, started with the core.
    Service,
    /// Finds things the core could add as sources, and says when they arrive
    /// and leave. One instance per plugin, as a service is.
    Device,
    /// Drives compositor pads over a take. One instance per plugin.
    Transition,
    /// A kind this core does not host: a panel, a surface, a preset, a
    /// graphic, a collection.
    Other,
}

impl ProvideKind {
    /// The word in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Output => "output",
            Self::Filter => "filter",
            Self::Service => "service",
            Self::Device => "device",
            Self::Transition => "transition",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> ProvideKind {
        match s {
            "source" => Self::Source,
            "output" => Self::Output,
            "filter" => Self::Filter,
            "service" => Self::Service,
            "device" => Self::Device,
            "transition" => Self::Transition,
            _ => Self::Other,
        }
    }

    /// True for the kinds the supervisor runs as one instance per plugin.
    pub fn is_singleton(self) -> bool {
        matches!(self, Self::Service | Self::Device | Self::Transition)
    }
}

/// Where an implementation runs. A built in kind is `Core`; the same trait
/// carries a crate compiled in by a custom build, and a process beside us.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    #[default]
    Core,
    InProcess,
    Sidecar,
    Node,
}

/// What a plugin delivers on each stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamMode {
    /// Frames or samples already at the canvas caps.
    Raw,
    /// A container the core demuxes and decodes.
    Container,
    /// Nothing on this stream. The harness expects no buffers.
    #[default]
    None,
}

impl StreamMode {
    pub fn present(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// The `media = { .. }` block of a `[[provides]]` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaDecl {
    pub video: StreamMode,
    pub audio: StreamMode,
    /// The provide emits AYUV and is composited over, not under, the
    /// programme layer.
    pub alpha: bool,
    /// The provide can build a thumbnail end when one is asked for.
    pub thumb: bool,
}

impl Default for MediaDecl {
    fn default() -> Self {
        Self { video: StreamMode::Raw, audio: StreamMode::Raw, alpha: false, thumb: true }
    }
}

/// What the supervisor is allowed to assume about a plugin.
///
/// The restart policy, the latency answer and the health reading are chosen
/// from these rather than from a flag on the core's own struct. That is the
/// whole difference between `superimposed()` deciding a rebuild and a plugin
/// saying whether it can come back in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// On a stall, NULL the pipeline and start it again. Without it the source
    /// is built from nothing, which is what a browser page gets.
    RestartInPlace,
    /// Trust the declared `latency_ms` when answering the LATENCY query.
    LatencyReport,
    /// The plugin answers `health` itself; its view is combined with the
    /// core's buffer observation.
    Health,
    /// The core forwards keyframe requests.
    KeyframeRequest,
    /// The source may be PAUSED when nothing is looking at it.
    Idle,
    /// `seek` and `position` are valid; the source gets a scrubber.
    Seek,
    /// `audio.set` accepts per layer levels.
    AudioLayers,
    /// The source emits AYUV and wants to be composited over the programme.
    Alpha,
}

impl Capability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RestartInPlace => "restart-in-place",
            Self::LatencyReport => "latency-report",
            Self::Health => "health",
            Self::KeyframeRequest => "keyframe-request",
            Self::Idle => "idle",
            Self::Seek => "seek",
            Self::AudioLayers => "audio-layers",
            Self::Alpha => "alpha",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.as_str() == s)
    }

    pub const ALL: [Capability; 8] = [
        Self::RestartInPlace,
        Self::LatencyReport,
        Self::Health,
        Self::KeyframeRequest,
        Self::Idle,
        Self::Seek,
        Self::AudioLayers,
        Self::Alpha,
    ];

    const fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// A set of capabilities, held as a bitset so the supervisor can ask its
/// question once per tick per source without allocating.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapabilitySet(u32);

impl CapabilitySet {
    pub const fn new() -> Self {
        Self(0)
    }

    pub const fn with(self, c: Capability) -> Self {
        Self(self.0 | c.bit())
    }

    pub const fn has(self, c: Capability) -> bool {
        self.0 & c.bit() != 0
    }

    pub fn set(&mut self, c: Capability, on: bool) {
        if on {
            self.0 |= c.bit();
        } else {
            self.0 &= !c.bit();
        }
    }

    /// The declared strings, in the order 03 section 4 lists them. What
    /// `plugin.describe` and the status extras report.
    pub fn names(self) -> Vec<&'static str> {
        Capability::ALL.iter().filter(|c| self.has(**c)).map(|c| c.as_str()).collect()
    }
}

/// What a plugin says about itself before anything is built.
///
/// The Rust mirror of one `[[provides]]` entry in `gmx-plugin.toml`. A built in
/// kind writes it as a constant; a sidecar's is made by the loader from the
/// plugin's own manifest and interned, which is why every field is a `'static`
/// borrow and the whole thing is `Copy`.
#[derive(Debug, Clone, Copy)]
pub struct Manifest {
    /// The namespace. Every id from this plugin is `<plugin>/<id>`.
    pub plugin: &'static str,
    /// The provide id within the plugin.
    pub id: &'static str,
    pub kind: ProvideKind,
    pub api: u32,
    pub description: &'static str,
    /// Schemes a bare URI can be matched against, GStreamer rank style.
    pub uri_schemes: &'static [&'static str],
    /// 0 to 256. The highest ranked provide claiming a scheme wins.
    pub rank: u16,
    pub media: MediaDecl,
    pub capabilities: CapabilitySet,
    /// What the plugin says its own delay is, used when it declares
    /// `latency-report`.
    pub latency_ms: u32,
    pub tier: Tier,
}

/// One kind, as a picker or a listing wants it.
///
/// Taken off the manifest and nothing else, so it costs no pipeline and can
/// be printed by `--api-info` on a machine with no GStreamer installed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KindInfo {
    /// The plugin qualified id, which is what goes in `type`.
    pub id: String,
    pub description: &'static str,
    /// URI schemes a bare address is matched against. Empty on a kind that
    /// has to be named outright.
    pub schemes: &'static [&'static str],
}

impl Manifest {
    /// What a picker shows for this provide.
    pub fn describe(&self) -> KindInfo {
        KindInfo {
            id: self.provide_id(),
            description: self.description,
            schemes: self.uri_schemes,
        }
    }

    /// The plugin qualified id an operator writes in `type`.
    pub fn provide_id(&self) -> String {
        format!("{}/{}", self.plugin, self.id)
    }

    /// True when `type = "<this>"` names this provide.
    pub fn is(&self, type_id: &str) -> bool {
        match type_id.split_once('/') {
            Some((p, i)) => p == self.plugin && i == self.id,
            // A bare name is read as the plugin's default provide of its kind.
            None => type_id == self.plugin,
        }
    }
}

/// What the core tells a plugin at `initialize`.
#[derive(Debug, Clone)]
pub struct Hello {
    pub instance: SourceId,
    pub canvas: crate::caps::CanvasCaps,
    pub api_level: u32,
    /// The `params` table from config, already merged with any legacy fields
    /// the migration table maps in.
    pub params: crate::config::Params,
    pub tier: Tier,
}

/// What a plugin answers with.
#[derive(Debug, Clone)]
pub struct Ready {
    pub manifest: Manifest,
    pub latency_ms: u32,
    pub capabilities: CapabilitySet,
}

/// The answer to `configure`. A plugin that cannot take a change while running
/// says so rather than crashing; the supervisor then rebuilds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Configure {
    Applied,
    RestartRequired(String),
}

/// The lifecycle states from 03 section 7. One enum for events, listings and
/// the harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginState {
    Starting,
    Ready,
    Running,
    Stalled,
    Degraded,
    Stopped,
    Failed,
}

/// A plugin's own view of itself, combined with the core's buffer observation.
#[derive(Debug, Clone)]
pub struct Health {
    pub state: PluginState,
    pub detail: Option<String>,
}

impl Health {
    pub fn running() -> Self {
        Self { state: PluginState::Running, detail: None }
    }

    pub fn of(state: PluginState) -> Self {
        Self { state, detail: None }
    }
}

/// What a source hands back: a pipeline of its own and the proxy sinks the
/// programme and the multiview attach to.
///
/// Exactly what `build_kind` produced before it was split, named. The
/// thumbnail end is optional because nothing builds a picture nobody is
/// looking at: it exists when `start` was called with `thumb = true`.
pub struct MediaEnds {
    pub pipeline: gst::Pipeline,
    pub video: gst::Element,
    pub audio: gst::Element,
    pub thumb: Option<gst::Element>,
    /// The canvas video capsfilter. Its src pad is the per source filter
    /// insertion point, and everything downstream of it is interchangeable.
    pub vcaps: gst::Element,
    /// The canvas audio capsfilter, the audio insertion point.
    pub acaps: gst::Element,
    /// The video tee. A thumbnail end is a branch off this, added and removed
    /// while the source runs without the programme branch noticing.
    pub vtee: gst::Element,
    /// The source's raw audio tee, where audio monitoring hangs off.
    pub atee: gst::Element,
    /// Liveness, marked from pad probes on the proxy sinks.
    pub health: Arc<SourceHealth>,
    pub last_video: Arc<crate::input::LastBuffer>,
    pub last_audio: Arc<crate::input::LastBuffer>,
    /// What the kind wants the core to hold on its behalf. Everything in here
    /// is kind specific and the core only stores it.
    pub parts: kinds::KindParts,
}

/// What this build can be asked to make: every source, output and filter type
/// id, with what the plugin behind it says it is.
///
/// The picker in the UI reads this out of `protocol.json` so that a build with
/// an extra kind offers it without the page being redeployed, and a build
/// without one does not offer a tile that would be refused. Static data off
/// the three registries: no pipeline is touched, which is why `--api-info` can
/// print it on a machine with no GStreamer.
///
/// It lives here rather than in `godwinmix-protocol` because what a build can
/// make is a property of the engine, not of the protocol.
pub fn described_kinds() -> serde_json::Value {
    serde_json::json!({
        "source": source::described(),
        "output": output::described(),
        "filter": filter::described(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capability_set_round_trips_through_its_names() {
        let caps = CapabilitySet::new()
            .with(Capability::RestartInPlace)
            .with(Capability::Seek)
            .with(Capability::Health);
        assert!(caps.has(Capability::Seek));
        assert!(!caps.has(Capability::Alpha));
        let names = caps.names();
        assert_eq!(names, vec!["restart-in-place", "health", "seek"]);
        let mut back = CapabilitySet::new();
        for n in names {
            back.set(Capability::parse(n).expect("a name the set just printed"), true);
        }
        assert_eq!(back, caps);
    }

    #[test]
    fn a_provide_id_is_matched_whole() {
        let m = Manifest {
            plugin: "file",
            id: "source",
            kind: ProvideKind::Source,
            api: API_LEVEL,
            description: "",
            uri_schemes: &["file://"],
            rank: 128,
            media: MediaDecl::default(),
            capabilities: CapabilitySet::new(),
            latency_ms: 0,
            tier: Tier::Core,
        };
        assert_eq!(m.provide_id(), "file/source");
        assert!(m.is("file/source"));
        assert!(m.is("file"));
        assert!(!m.is("file/output"));
        assert!(!m.is("filesource"));
    }
}
