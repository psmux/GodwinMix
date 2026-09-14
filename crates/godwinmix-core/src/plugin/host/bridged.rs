//! `BridgedSource`: a source whose process is on another machine.
//!
//! One more implementation of the same `Source` trait `videotestsrc` and
//! `SidecarSource` implement. The mixer cannot tell the difference and that is
//! the whole point: a remote plugin's settings form, tools, health and events
//! are the local ones, because they are answered by the same plugin over a
//! different carrier.
//!
//! What is different from a sidecar, and only this:
//!
//! * The process is spawned by the node, not here. `start` asks the node to
//!   spawn it and the node runs the identical tier 2 host.
//! * Control goes over the node's WebSocket, tagged with `instance`, instead
//!   of down a pipe.
//! * Media arrives already encoded over RTP, SRT or WHIP, and is decoded here
//!   on the core's usual hardware aware path. The thumbnail is made here from
//!   the received feed, so nothing sends a second stream for a picture.
//! * The latency budget is declared at ingress and answered on the LATENCY
//!   query, which is what keeps two remote cameras in lip sync with a local
//!   file.

use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::node::bridge::Peer;
use crate::node::media::{self, Receiver};
use crate::node::wire::{MediaPlan, Spawn};
use crate::plugin::kinds::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::plugin::source::{unknown_method, Source};
use crate::plugin::{Configure, Health, Hello, Manifest, MediaEnds, PluginState, Ready};
use anyhow::{Context, Result};
use godwinmix_protocol::plugin::wire::{Canvas, HealthState};
use gstreamer as gst;
use serde_json::{json, Value};
use std::sync::Arc;

/// Everything the core needs to put one instance on a node.
pub struct BridgedSpec {
    /// The node's name, as the operator wrote it in `place`.
    pub node: String,
    /// `<plugin>/<provide>`.
    pub type_id: String,
    /// The interned manifest for that provide, which came off the node's
    /// hello. Identical in shape to a local one, so nothing downstream has a
    /// second path.
    pub manifest: Manifest,
    /// The live bridge. Taken again on every `start`, because a node that
    /// reconnected has a new one.
    pub link: Arc<dyn LinkSource>,
    /// How the media travels.
    pub plan: MediaPlan,
}

/// Where a bridged instance gets its node's socket from.
///
/// A trait rather than the registry itself, so the host does not depend on the
/// node server and a test can hand it a socket with nothing behind it.
pub trait LinkSource: Send + Sync {
    fn link(&self, node: &str) -> Option<Arc<Peer>>;
    /// Run one call on the bridge from a blocking context.
    ///
    /// Every `Source` method is synchronous, because a mixer command is, and
    /// the bridge is async. This is the one place the two meet, and it is a
    /// `block_on` on a runtime handle rather than on the mixer thread.
    fn call(&self, node: &str, method: &str, params: Value) -> Result<Value>;
}

pub struct BridgedSource {
    spec: BridgedSpec,
    build: BuildCtx,
    /// The receive elements, kept so `stop` can find them again.
    receiver: Option<Receiver>,
    latency_ms: u32,
    started: bool,
}

impl BridgedSource {
    pub fn new(spec: BridgedSpec, build: BuildCtx) -> Self {
        let latency_ms = spec.plan.latency_ms;
        Self { spec, build, receiver: None, latency_ms, started: false }
    }

    pub fn node(&self) -> &str {
        &self.spec.node
    }

    pub fn plan(&self) -> &MediaPlan {
        &self.spec.plan
    }

    pub fn latency_ms(&self) -> u32 {
        self.latency_ms
    }

    /// Whether the node this instance is on is answering right now.
    pub fn reachable(&self) -> bool {
        self.spec.link.link(&self.spec.node).is_some()
    }

    fn call_node(&self, method: &str, mut params: Value) -> Result<Value> {
        if let Some(map) = params.as_object_mut() {
            map.insert("instance".into(), json!(self.build.id.as_str()));
        }
        self.spec.link.call(&self.spec.node, method, params)
    }

    /// Ask the node to start the plugin and point its media here.
    fn spawn_remote(&mut self, canvas: &CanvasCaps) -> Result<()> {
        let spawn = Spawn {
            instance: self.build.id.to_string(),
            type_id: self.spec.type_id.clone(),
            params: serde_json::to_value(self.build.cfg.effective_params())
                .unwrap_or(Value::Null),
            canvas: Canvas {
                width: canvas.width as u32,
                height: canvas.height as u32,
                fps: (canvas.fps.numer() as u32) / (canvas.fps.denom().max(1) as u32),
            },
            media: self.spec.plan.clone(),
        };
        let answer = self
            .spec
            .link
            .call(&self.spec.node, "node.spawn", serde_json::to_value(&spawn)?)
            .with_context(|| {
                format!(
                    "`{}` would not start `{}` for this source",
                    self.spec.node, self.spec.type_id
                )
            })?;
        if let Some(ms) = answer.get("latency_ms").and_then(Value::as_u64) {
            self.latency_ms = ms as u32;
        }
        crate::plugin::loader::set_state(
            self.build.id.as_str(),
            godwinmix_protocol::plugin::wire::InstanceState::Running.as_str(),
        );
        crate::plugin::loader::set_latency(self.build.id.as_str(), self.latency_ms);
        Ok(())
    }
}

impl Source for BridgedSource {
    fn manifest(&self) -> &Manifest {
        &self.spec.manifest
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.build.canvas = hello.canvas;
        self.build.cfg.params = hello.params.clone();
        // Nothing is spawned here. A remote instance is started by `start`,
        // because the node has to be told where to send the media and the plan
        // is only settled once. A sidecar shakes hands early because the
        // handshake picks its transport; a node's transport is config.
        Ok(Ready {
            manifest: self.spec.manifest,
            latency_ms: self.latency_ms,
            capabilities: self.spec.manifest.capabilities,
        })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.build.canvas = canvas.clone();
        let id = self.build.id.clone();
        let receiver = media::receiver(id.as_str(), &self.spec.plan).with_context(|| {
            format!("build the receive side for `{id}` from {}", self.spec.node)
        })?;
        let plan = self.spec.plan.clone();
        let elements = receiver.elements.clone();
        let decode = receiver.decode.clone();
        let for_wiring = Receiver { elements: elements.clone(), decode: decode.clone() };
        // The declared budget is what the pipeline answers a LATENCY query
        // with, so every sink downstream delays by the same amount and lip
        // sync between two remote cameras holds. Without it the join is
        // claimed as zero and the second camera is early by its jitter buffer.
        let ends = assemble(
            &self.build,
            thumb,
            Ingest::default().with(elements).livesync(false),
            |w: &Wiring| {
                media::link_receiver(&for_wiring, &plan)?;
                w.route(&decode, w.norm.video_entry(), w.norm.audio_entry());
                Ok(KindParts::default())
            },
        )?;
        // The budget is declared where the media arrives, before anything
        // downstream has had a chance to assume zero. The receive element does
        // the actual buffering (rtpbin's jitter buffer, or SRT's latency); this
        // is what makes the figure visible to every sink.
        if let Some(first) = receiver.elements.first() {
            crate::gstutil::declare_latency(first, self.spec.plan.latency_ms)?;
        }
        self.receiver = Some(receiver);
        self.spawn_remote(canvas)?;
        self.started = true;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.started = false;
        // A node that has gone cannot be told to stop, and that is fine: it
        // has nothing running either. The error is logged rather than returned
        // so removing a source never fails because a machine is off.
        if let Err(e) =
            self.call_node("node.stop", json!({ "reason": "the source was stopped" }))
        {
            tracing::debug!(
                instance = %self.build.id,
                node = %self.spec.node,
                error = %format!("{e:#}"),
                "the node could not be told to stop this source"
            );
        }
        crate::plugin::loader::set_state(
            self.build.id.as_str(),
            godwinmix_protocol::plugin::wire::InstanceState::Stopped.as_str(),
        );
        self.receiver = None;
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        let answer = self.call_node(
            "configure",
            json!({ "params": serde_json::to_value(params).unwrap_or(Value::Null) }),
        )?;
        if answer.get("applied").and_then(Value::as_bool).unwrap_or(false) {
            return Ok(Configure::Applied);
        }
        let reason = answer
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("the plugin did not say why")
            .to_string();
        Ok(Configure::RestartRequired(reason))
    }

    fn health(&self) -> Health {
        if !self.started {
            return Health::of(PluginState::Stopped);
        }
        // Layer one of the three in 04 section 5: the node is not answering,
        // so nothing it hosts is running, whatever the plugin last said.
        if !self.reachable() {
            return Health {
                state: PluginState::Failed,
                detail: Some(format!(
                    "the node `{}` is not answering. The programme holds the freeze frame, then \
                     the slate; the source comes back on its own when the node does",
                    self.spec.node
                )),
            };
        }
        if !self.spec.manifest.capabilities.has(crate::plugin::Capability::Health) {
            return Health::of(PluginState::Running);
        }
        match self.call_node("health", json!({})) {
            Ok(value) => {
                let health: godwinmix_protocol::plugin::wire::Health =
                    serde_json::from_value(value).unwrap_or_default();
                Health {
                    state: match health.state {
                        HealthState::Ok => PluginState::Running,
                        HealthState::Degraded => PluginState::Degraded,
                        HealthState::Failing => PluginState::Failed,
                    },
                    detail: health.detail,
                }
            }
            Err(e) => Health {
                state: PluginState::Degraded,
                detail: Some(format!("the node did not answer `health`: {e}")),
            },
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            // A restart of a remote instance is the node stopping and starting
            // the process. The receive side stays: the ports are the same and
            // the pipeline here does not know anything happened.
            "restart" => {
                self.call_node("node.stop", json!({ "reason": "restart" }))?;
                crate::plugin::loader::count_restart(self.build.id.as_str());
                let canvas = self.build.canvas.clone();
                self.spawn_remote(&canvas)?;
                Ok(json!({ "respawned": true }))
            }
            "seek" | "position" | "keyframe" | "audio.set" | "tool.call" | "discover"
            | "render" => self.call_node(method, params),
            other => Err(unknown_method(
                &self.spec.manifest,
                other,
                &["restart", "seek", "position", "keyframe", "audio.set", "tool.call"],
            )),
        }
    }
}

impl Drop for BridgedSource {
    fn drop(&mut self) {
        if self.started {
            let _ = self.call_node("node.stop", json!({ "reason": "the source was removed" }));
        }
    }
}

/// Build a source that runs on a node.
///
/// Refuses, by name, every way this can be wrong before anything is built: a
/// core with no node machinery, a node nobody has enrolled, a plugin that node
/// has not got, and a plugin that did not declare the `node` placement. Each
/// refusal names what would have worked, because an operator who wrote
/// `place = "node:studio-b"` and got "no" needs to know which of those it was.
pub fn make(
    req: crate::plugin::source::SourceRequest<'_>,
    type_id: &str,
    node: &str,
) -> Result<Box<dyn Source>> {
    let runtime = crate::node::runtime::get().context(
        "this core has no node bridge, so nothing can be placed on a node. Add a [nodes] table \
         to the config and restart",
    )?;
    anyhow::ensure!(
        runtime.nodes.view(node).is_some(),
        "no node called `{node}`. This core knows: {}. `gmx node token --name {node}` mints an \
         enrolment token for a new one",
        match runtime.nodes.names().join(", ") {
            names if names.is_empty() => "none".to_string(),
            names => names,
        }
    );
    // The manifest comes off the node's hello, so it is the plugin that will
    // actually run rather than whatever this machine happens to have installed
    // under the same name.
    let plugin = crate::plugin::remote::plugin_manifest(type_id, Some(node)).or_else(|| {
        crate::plugin::loader::get(type_id.split('/').next().unwrap_or(type_id))
            .map(|p| p.manifest)
    });
    let plugin = plugin.with_context(|| {
        format!(
            "`{node}` does not have `{type_id}`. It has: {}",
            match crate::plugin::remote::on_node(node)
                .iter()
                .map(|p| p.plugin.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
            {
                names if names.is_empty() => "nothing it has told us about".to_string(),
                names => names,
            }
        )
    })?;
    crate::node::check_placement(
        type_id,
        &crate::node::Place::Node(node.to_string()),
        &plugin.plugin.placements,
    )?;
    let manifest = crate::plugin::remote::manifest(type_id)
        .or_else(|| crate::plugin::loader::provide_manifest(type_id))
        .with_context(|| format!("`{type_id}` has no manifest on `{node}` or here"))?;
    let plan = runtime.plan(
        node,
        req.cfg.bridge_transport(),
        req.cfg.latency_ms,
    )?;
    let mut build = req.ctx();
    build.tier = crate::plugin::Tier::Node;
    Ok(Box::new(BridgedSource::new(
        BridgedSpec {
            node: node.to_string(),
            type_id: type_id.to_string(),
            manifest: *manifest,
            link: runtime,
            plan,
        },
        build,
    )))
}

/// Elements the receive side wants in the pipeline, for a caller that builds
/// its own. Used by the move under the freeze frame, which needs the new side
/// running before the old one stops.
pub fn receive_elements(id: &str, plan: &MediaPlan) -> Result<Vec<gst::Element>> {
    Ok(media::receiver(id, plan)?.elements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::wire::BridgeTransport;

    /// A link that answers nothing, for the paths that do not need a node.
    struct NoLink;

    impl LinkSource for NoLink {
        fn link(&self, _: &str) -> Option<Arc<Peer>> {
            None
        }

        fn call(&self, node: &str, method: &str, _: Value) -> Result<Value> {
            anyhow::bail!("`{method}` cannot go anywhere: `{node}` is not connected")
        }
    }

    fn manifest() -> Manifest {
        crate::plugin::kinds::testsrc::PROVIDE.manifest
    }

    fn source() -> BridgedSource {
        let cfg = crate::config::SourceConfig::bare("cam1", "");
        let canvas = CanvasCaps::new(&crate::config::Canvas::default());
        BridgedSource::new(
            BridgedSpec {
                node: "studio-b".into(),
                type_id: "ndi/source".into(),
                manifest: manifest(),
                link: Arc::new(NoLink),
                plan: MediaPlan {
                    transport: BridgeTransport::Srt,
                    target: "srt://10.0.0.21:8500".into(),
                    audio_port: 0,
                    latency_ms: 120,
                },
            },
            BuildCtx {
                id: cfg.id.clone().into(),
                cfg,
                canvas,
                backends: crate::probe::Backends::from_selection(
                    &crate::catalogue::select(
                        &toml::from_str::<crate::config::Config>("").unwrap(),
                        None,
                    )
                    .expect("select a codec for the test"),
                ),
                thumb_fps: 1,
                browser: Default::default(),
                allow_exec: false,
                origin: std::time::Instant::now(),
                tier: crate::plugin::Tier::Node,
            },
        )
    }

    #[test]
    fn an_unreachable_node_makes_its_sources_fail_with_the_reason() {
        gst::init().unwrap();
        let mut bridged = source();
        // Not started: stopped, not failed. A source nobody asked for is not a
        // fault.
        assert_eq!(bridged.health().state, PluginState::Stopped);
        bridged.started = true;
        let health = bridged.health();
        assert_eq!(health.state, PluginState::Failed);
        let detail = health.detail.unwrap_or_default();
        assert!(detail.contains("studio-b"), "{detail}");
        assert!(detail.contains("freeze frame"), "the reason must say what the viewer sees: {detail}");
        bridged.started = false;
    }

    #[test]
    fn it_reports_the_remote_plugins_manifest_and_not_its_own() {
        gst::init().unwrap();
        let bridged = source();
        assert_eq!(bridged.manifest().plugin, manifest().plugin);
        assert_eq!(bridged.latency_ms(), 120);
        assert_eq!(bridged.node(), "studio-b");
    }

    #[test]
    fn a_method_the_plugin_does_not_answer_lists_what_it_does() {
        gst::init().unwrap();
        let mut bridged = source();
        let e = bridged.call("fly", Value::Null).unwrap_err().to_string();
        assert!(e.contains("seek"), "{e}");
        bridged.started = false;
    }

    #[test]
    fn stopping_a_source_on_a_node_that_is_gone_still_succeeds() {
        gst::init().unwrap();
        let mut bridged = source();
        bridged.started = true;
        assert!(bridged.stop().is_ok(), "removing a source must not fail because a machine is off");
    }
}
