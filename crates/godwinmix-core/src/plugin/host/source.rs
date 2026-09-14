//! `SidecarSource`: a source that lives in another process.
//!
//! One more implementation of the same `Source` trait every built in kind
//! implements. It spawns the process, runs the handshake, and wires the
//! transport it settled on into the same `MediaEnds` that `videotestsrc`
//! produces. That is the whole trick of the tier system: the mixer never
//! learns which tier it is talking to, so a crash costs one source and the
//! supervisor above it needs no new code.

use super::process::{Notice, Sidecar};
use super::transport::{self, MediaDir};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::input::attach_exec_stdout;
use crate::plugin::kinds::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::plugin::source::{unknown_method, Source};
use crate::plugin::{
    Configure, Health, Hello, Manifest, MediaEnds, PluginState, Ready,
};
use anyhow::{Context, Result};
use godwinmix_host::launch::{Launch, LaunchCtx};
use godwinmix_protocol::plugin::manifest::Manifest as PluginManifest;
use godwinmix_protocol::plugin::wire::{Canvas, HealthState, InstanceState, Transport};
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use tracing::{debug, warn};

/// Everything a sidecar kind needs that the core's own kinds get from a
/// `BuildCtx`, plus the plugin's own manifest and where it is installed.
pub struct SidecarSpec {
    /// The `gmx-plugin.toml` this instance came from.
    pub plugin: PluginManifest,
    /// The provide id within the plugin.
    pub provide: String,
    /// The interned manifest the core's registries hold for this provide.
    pub manifest: Manifest,
    pub launch: Launch,
    pub ctx: LaunchCtx,
    /// The canvas the instance was told about. Held on the spec because an
    /// output has no `BuildCtx` to keep it in.
    pub canvas: CanvasCaps,
    /// Where sockets go. A container instance never makes one.
    pub runtime: PathBuf,
}

pub struct SidecarSource {
    spec: SidecarSpec,
    build: BuildCtx,
    child: Option<Sidecar>,
    media: Option<MediaDir>,
    /// The source element on the container path, kept so a restart in place
    /// can hand it a new pipe. `None` on a socket transport, where a restart
    /// reconnects the socket rather than changing a descriptor.
    element: Option<gst::Element>,
    latency_ms: u32,
    started: bool,
}

impl SidecarSource {
    pub fn new(spec: SidecarSpec, build: BuildCtx) -> Self {
        Self {
            spec,
            build,
            child: None,
            media: None,
            element: None,
            latency_ms: 0,
            started: false,
        }
    }

    /// What the plugin's own process is doing, for `plugin.list` and the
    /// supervisor.
    pub fn instance_state(&self) -> InstanceState {
        self.child.as_ref().map(Sidecar::state).unwrap_or(InstanceState::Stopped)
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(Sidecar::pid)
    }

    pub fn transport(&self) -> Option<Transport> {
        self.child.as_ref().and_then(Sidecar::transport)
    }

    /// Everything the plugin has said since this was last asked. The
    /// supervisor turns these into events, alerts and log lines; nothing is
    /// acted on inside the reader thread.
    pub fn notices(&self) -> Vec<Notice> {
        self.child.as_ref().map(Sidecar::drain).unwrap_or_default()
    }

    /// Forward a log level change so `log.set {instance, level}` reaches the
    /// plugin's own output.
    pub fn configure_log(&self, level: &str) -> Result<()> {
        self.child
            .as_ref()
            .context("the plugin is not running, so there is nothing to tell")?
            .configure_log(level)
    }

    /// Start the process again with the same command line. What
    /// `restart-in-place` means for a sidecar: the pipeline stays, the process
    /// behind it changes.
    pub fn respawn(&mut self) -> Result<()> {
        let id = self.build.id.clone();
        if let Some(old) = self.child.as_mut() {
            old.lifecycle_mut().restarting();
            old.shutdown("restarting");
        }
        crate::plugin::loader::count_restart(&id);
        self.child = None;
        self.handshake()?;
        // The element stays and the pipe behind it changes, which is what
        // `restart-in-place` means and what the exec kind already does. The
        // element belongs to the pipeline the mixer holds, so it is found by
        // name there rather than kept here: a kind that held a reference to it
        // would keep the old pipeline alive after a rebuild.
        if self.started {
            let src = self.element.clone();
            if let (Some(child), Some(src)) = (self.child.as_mut(), src) {
                if let Some(out) = child.take_stdout() {
                    let held = attach_exec_stdout(&id, &src, out);
                    child.hold_stdout(held);
                }
            }
            self.call_start()?;
        }
        Ok(())
    }

    /// Spawn and shake hands. Called from `initialize`, because the answer
    /// decides the transport and the transport decides what `start` builds.
    fn handshake(&mut self) -> Result<()> {
        let instance = self.build.id.clone();
        let mut child = Sidecar::spawn(&instance, &self.spec.launch)
            .with_context(|| format!("starting `{}`", self.spec.launch.command_line()))?;
        let canvas = canvas_of(&self.build.canvas);
        let params = params_json(&self.build.cfg.effective_params());
        let runtime = self.spec.runtime.clone();
        let mut made: Option<MediaDir> = None;
        let outcome = child.handshake(
            Some(&self.spec.plugin),
            canvas,
            &self.spec.provide,
            params,
            |t| {
                transport::usable(t)?;
                let dir = MediaDir::create(&runtime, &instance)?;
                let base = dir.base();
                made = Some(dir);
                Ok(base)
            },
        );
        match outcome {
            Ok(negotiated) => {
                self.latency_ms = negotiated.latency_ms.unwrap_or(self.spec.manifest.latency_ms);
            }
            Err(e) => {
                // The handshake is the one place a plugin is killed rather
                // than restarted: a process that cannot say hello will not say
                // it on the second try either, and the reason goes to
                // `event/plugin.state`.
                child.lifecycle_mut().failed(e.to_string());
                child.shutdown("the handshake failed");
                return Err(e);
            }
        }
        // The loader holds the numbers `plugin.list` and `plugin.stats` carry,
        // and the sampler reads a set of pids at a time. Telling it which
        // process is behind this instance is what puts a cost next to the
        // plugin's name.
        crate::plugin::loader::set_pid(
            &instance,
            &self.spec.plugin.plugin.name,
            &self.spec.provide,
            child.pid(),
        );
        crate::plugin::loader::set_state(&instance, child.state().as_str());
        crate::plugin::loader::set_latency(&instance, self.latency_ms);
        self.media = made;
        self.child = Some(child);
        Ok(())
    }

    /// Tell the plugin to start producing, once the pipeline is built.
    fn call_start(&mut self) -> Result<()> {
        let (transport, media) = {
            let child = self.child.as_ref().context("the plugin is not running")?;
            (child.transport().unwrap_or(Transport::Container), child.media_address().to_string())
        };
        let params = json!({
            "canvas": canvas_of(&self.build.canvas),
            "transport": transport,
            "media": media,
        });
        let answer = self
            .child
            .as_ref()
            .expect("checked above")
            .call("start", params)
            .context("the plugin refused to start")?;
        if let Some(ms) = answer.get("latency_ms").and_then(Value::as_u64) {
            self.latency_ms = ms as u32;
        }
        self.started = true;
        crate::plugin::loader::set_latency(&self.build.id, self.latency_ms);
        crate::plugin::loader::set_state(
            &self.build.id,
            godwinmix_protocol::plugin::wire::InstanceState::Running.as_str(),
        );
        Ok(())
    }

    /// The elements upstream of the normaliser, for the transport that was
    /// settled on.
    fn ingest(&mut self) -> Result<(Ingest, Wire)> {
        let id = self.build.id.clone();
        let canvas = self.build.canvas.clone();
        let transport = self
            .child
            .as_ref()
            .and_then(Sidecar::transport)
            .context("nothing has been negotiated; the handshake has not run")?;
        match transport {
            Transport::Container => {
                let src = crate::gstutil::make(
                    if cfg!(unix) { "fdsrc" } else { "appsrc" },
                    &format!("{id}-src-container"),
                )?;
                if cfg!(unix) {
                    // Frame sized reads rather than the 4 KB default. A source
                    // writing raw video moves a lot through that pipe, and the
                    // difference is whether it keeps up.
                    crate::probe::set_int(&src, "blocksize", 4 * 1024 * 1024);
                }
                let decode = transport::decoder(&id)?;
                // A hardware decoder may hand out frames in device memory, and
                // the normaliser wants them in system memory. `decodebin`
                // usually inserts the download itself, and on macOS with an FLV
                // it does not: it autoplugs `vtdec_hw`, which negotiates GL
                // memory against the normaliser and then never produces a
                // frame. Measured with gst-launch alone, so it is the elements
                // and not the core. Matroska and MPEG-TS are unaffected, which
                // is why this was invisible until a plugin wrote FLV.
                //
                // The built in `rtmp/source` has always put one here for the
                // same reason. Absent is not fatal: caps negotiation inserts
                // one for the containers it can.
                let download = match self.build.backends.video_decode.download {
                    Some(f) if crate::probe::exists(f) => {
                        Some(crate::gstutil::make(f, &format!("{id}-vdl"))?)
                    }
                    Some(f) => {
                        debug!(
                            source = %id,
                            element = f,
                            "no download element on this machine; relying on caps negotiation"
                        );
                        None
                    }
                    None => None,
                };
                let mut elements = vec![src.clone(), decode.clone()];
                elements.extend(download.clone());
                Ok((
                    Ingest::default().with(elements).livesync(false),
                    Wire::Container { src, decode, download },
                ))
            }
            socket => {
                let media = self.media.as_ref().context("a socket transport with no address")?;
                let video = transport::socket_video(socket, &id, &media.video(), &canvas)?;
                let declared = self.spec.manifest.media;
                let audio = declared
                    .audio
                    .present()
                    .then(|| transport::socket_audio(socket, &id, &media.audio(), &canvas))
                    .transpose()?;
                let mut elements = video.clone();
                if let Some(a) = &audio {
                    elements.extend(a.clone());
                }
                Ok((
                    Ingest::default().with(elements).livesync(false),
                    Wire::Socket { video, audio },
                ))
            }
        }
    }
}

/// What has to be linked once everything is in one pipeline.
enum Wire {
    Container { src: gst::Element, decode: gst::Element, download: Option<gst::Element> },
    Socket { video: Vec<gst::Element>, audio: Option<Vec<gst::Element>> },
}

impl Source for SidecarSource {
    fn manifest(&self) -> &Manifest {
        &self.spec.manifest
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.build.canvas = hello.canvas;
        self.build.cfg.params = hello.params.clone();
        self.handshake()?;
        Ok(Ready {
            manifest: self.spec.manifest,
            latency_ms: self.latency_ms,
            capabilities: self.spec.manifest.capabilities,
        })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.build.canvas = canvas.clone();
        if self.child.is_none() {
            self.handshake()?;
        }
        let id = self.build.id.clone();
        let (ingest, wire) = self.ingest()?;
        let ends = assemble(&self.build, thumb, ingest, |w: &Wiring| match &wire {
            Wire::Container { src, decode, download } => {
                gst::Element::link(src, decode).context("linking the plugin's pipe to decodebin")?;
                match download {
                    Some(d) => {
                        gst::Element::link(d, &w.norm.video_entry())
                            .context("linking the download to the normaliser")?;
                        w.route(decode, d.clone(), w.norm.audio_entry());
                    }
                    None => w.route(decode, w.norm.video_entry(), w.norm.audio_entry()),
                }
                Ok(KindParts::default())
            }
            Wire::Socket { video, audio } => {
                gst::Element::link_many(video.iter().collect::<Vec<_>>().as_slice())
                    .context("linking the plugin's video socket")?;
                let last = video.last().context("an empty video chain")?;
                last.link(&w.norm.video_entry()).context("linking the plugin's video")?;
                w.has_video.store(true, std::sync::atomic::Ordering::Relaxed);
                if let Some(audio) = audio {
                    gst::Element::link_many(audio.iter().collect::<Vec<_>>().as_slice())
                        .context("linking the plugin's audio socket")?;
                    let last = audio.last().context("an empty audio chain")?;
                    last.link(&w.norm.audio_entry()).context("linking the plugin's audio")?;
                    w.has_audio.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Ok(KindParts::default())
            }
        })?;
        // The pipe is attached after the element is in a pipeline, which is
        // what `fdsrc` wants, and before the plugin is told to start, so
        // nothing it writes is lost.
        if let Wire::Container { src, .. } = &wire {
            self.element = Some(src.clone());
            if let Some(child) = self.child.as_mut() {
                if let Some(out) = child.take_stdout() {
                    let held = attach_exec_stdout(&id, src, out);
                    child.hold_stdout(held);
                }
            }
        }
        self.call_start()?;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.started = false;
        if let Some(child) = self.child.as_mut() {
            child.shutdown("the source was stopped");
        }
        crate::plugin::loader::set_pid(
            &self.build.id,
            &self.spec.plugin.plugin.name,
            &self.spec.provide,
            None,
        );
        crate::plugin::loader::set_state(
            &self.build.id,
            godwinmix_protocol::plugin::wire::InstanceState::Stopped.as_str(),
        );
        self.child = None;
        // Dropping it removes the directory, so no socket and no directory is
        // left behind. The leak counting test checks exactly that.
        self.media = None;
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        let child = self.child.as_ref().context(
            "the plugin is not running, so there is nothing to configure. Start the source \
             first, or call plugin.reload.",
        )?;
        let answer = child.call("configure", json!({ "params": params_json(params) }))?;
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
        let Some(child) = self.child.as_ref() else {
            return Health::of(PluginState::Stopped);
        };
        // A plugin that does not declare `health` is judged on its buffers
        // alone, which is what the supervisor was doing before plugins
        // existed. Asking one that never answers would cost a timeout a
        // second, every second.
        if !self.spec.manifest.capabilities.has(crate::plugin::Capability::Health) {
            return Health::of(state_of(child.state()));
        }
        match child.health() {
            Ok(h) => Health {
                state: match h.state {
                    HealthState::Ok => state_of(child.state()),
                    HealthState::Degraded => PluginState::Degraded,
                    HealthState::Failing => PluginState::Failed,
                },
                detail: h.detail,
            },
            Err(e) => {
                debug!(instance = %self.build.id, ?e, "the plugin did not answer `health`");
                Health {
                    state: PluginState::Degraded,
                    detail: Some("it did not answer `health` in time".into()),
                }
            }
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            "restart" => {
                self.respawn()?;
                Ok(json!({ "respawned": true }))
            }
            // Everything a plugin may implement goes straight through. The
            // legal call order is enforced by the lifecycle, so a `seek` on a
            // plugin that is not running is refused with the state named
            // rather than sent into a pipe nobody is reading.
            "seek" | "position" | "keyframe" | "audio.set" | "tool.call" | "discover"
            | "render" => {
                let child = self.child.as_ref().context("the plugin is not running")?;
                child.call(method, params)
            }
            other => Err(unknown_method(
                &self.spec.manifest,
                other,
                &["restart", "seek", "position", "keyframe", "audio.set", "tool.call"],
            )),
        }
    }
}

impl Drop for SidecarSource {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            child.shutdown("the source was removed");
        }
    }
}

/// The lifecycle state as the core's own enum spells it.
pub fn state_of(state: InstanceState) -> PluginState {
    match state {
        InstanceState::Starting => PluginState::Starting,
        InstanceState::Ready => PluginState::Ready,
        InstanceState::Running => PluginState::Running,
        InstanceState::Stalled => PluginState::Stalled,
        InstanceState::Degraded | InstanceState::OverBudget => PluginState::Degraded,
        InstanceState::Stopped => PluginState::Stopped,
        InstanceState::Failed => PluginState::Failed,
    }
}

/// The canvas as the wire spells it.
pub fn canvas_of(canvas: &CanvasCaps) -> Canvas {
    Canvas {
        width: canvas.width as u32,
        height: canvas.height as u32,
        fps: (canvas.fps.numer() as u32) / (canvas.fps.denom().max(1) as u32),
    }
}

/// A TOML params table as the JSON a plugin reads.
pub fn params_json(params: &Params) -> Value {
    serde_json::to_value(params).unwrap_or_else(|e| {
        warn!(?e, "a params table would not convert to JSON; sending an empty one");
        json!({})
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_canvas_crosses_the_boundary_as_three_numbers() {
        let canvas = CanvasCaps::new(&crate::config::Canvas {
            width: 1280,
            height: 720,
            fps: 30,
            sample_rate: 48000,
            channels: 2,
        });
        let wire = canvas_of(&canvas);
        assert_eq!((wire.width, wire.height, wire.fps), (1280, 720, 30));
    }

    #[test]
    fn every_lifecycle_state_has_a_core_state() {
        for (from, want) in [
            (InstanceState::Starting, PluginState::Starting),
            (InstanceState::Running, PluginState::Running),
            (InstanceState::OverBudget, PluginState::Degraded),
            (InstanceState::Failed, PluginState::Failed),
        ] {
            assert_eq!(state_of(from), want, "{from:?}");
        }
    }

    #[test]
    fn a_params_table_becomes_the_object_a_plugin_reads() {
        let mut params = Params::new();
        params.insert("timezone".into(), toml::Value::String("Europe/London".into()));
        params.insert("size".into(), toml::Value::Integer(48));
        let json = params_json(&params);
        assert_eq!(json["timezone"], "Europe/London");
        assert_eq!(json["size"], 48);
    }
}
