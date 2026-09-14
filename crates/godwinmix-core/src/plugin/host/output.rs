//! `SidecarOutput`: an output that lives in another process.
//!
//! The programme is already encoded and muxed by the time it reaches an
//! output, so a sidecar one is the simplest of the five: the core writes a
//! streamable container and the plugin sends or stores it. A plugin that
//! declared raw media gets raw instead, at canvas caps, down the same path.
//!
//! One thing is different here from a source, and it is worth saying plainly.
//! A source puts media on its stdout and keeps stdin for the control channel,
//! which is what 03 section 6 describes. An output's media travels the other
//! way, and stdin is already the control channel: a pipe carries bytes in one
//! direction, and JSON lines and a Matroska stream cannot share one. So an
//! output's media goes to the address in `GMX_MEDIA`, which for the container
//! transport is a FIFO the core makes beside the instance's sockets, and the
//! control channel stays exactly where it is for every other kind. One
//! protocol, one channel, and the media on the transport that was negotiated,
//! which is the rule everywhere else too.
//!
//! A FIFO is a Unix thing. On Windows an output sidecar is refused with a
//! message naming the limitation rather than half working; the named pipe that
//! would fix it is `docs/reference/plugin-lifecycle.md`'s open question.

use super::process::Sidecar;
use super::source::{canvas_of, params_json, state_of, SidecarSpec};
use super::transport::MediaDir;
use crate::config::Params;
use crate::gstutil::make;
use crate::plugin::output::{Output, OutputCtx};
use crate::plugin::source::unknown_method;
use crate::plugin::{Configure, Health, Hello, Manifest, PluginState, Ready, StreamMode};
use anyhow::{Context, Result};
use godwinmix_protocol::plugin::wire::{HealthState, InstanceState};
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};
use tracing::debug;

pub struct SidecarOutput {
    spec: SidecarSpec,
    child: Option<Sidecar>,
    media: Option<MediaDir>,
    id: String,
    started: bool,
}

impl SidecarOutput {
    pub fn new(spec: SidecarSpec) -> Self {
        Self { spec, child: None, media: None, id: String::new(), started: false }
    }

    pub fn instance_state(&self) -> InstanceState {
        self.child.as_ref().map(Sidecar::state).unwrap_or(InstanceState::Stopped)
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(Sidecar::pid)
    }

    fn handshake(&mut self, instance: &str, params: &Params) -> Result<()> {
        anyhow::ensure!(
            cfg!(unix),
            "an output plugin needs a FIFO to receive the programme on, and this is {}. \
             Sidecar outputs are Unix only for now; a first party output (rtmp/output, \
             srt/output) works everywhere.",
            std::env::consts::OS
        );
        let dir = MediaDir::create(&self.spec.runtime, instance)?;
        let fifo = dir.programme();
        make_fifo(&fifo)?;
        let mut launch = self.spec.launch.clone();
        launch.env.insert("GMX_MEDIA".into(), fifo.clone());
        let mut child = Sidecar::spawn(instance, &launch)
            .with_context(|| format!("starting `{}`", launch.command_line()))?;
        let canvas = canvas_of(&self.spec.canvas);
        let outcome = child.handshake(
            Some(&self.spec.plugin),
            canvas,
            &self.spec.provide,
            params_json(params),
            |_| Ok(fifo.clone()),
        );
        if let Err(e) = outcome {
            child.lifecycle_mut().failed(e.to_string());
            child.shutdown("the handshake failed");
            return Err(e);
        }
        self.media = Some(dir);
        self.child = Some(child);
        Ok(())
    }
}

/// Make a FIFO, or say plainly that this platform has none.
#[cfg(unix)]
fn make_fifo(path: &str) -> Result<()> {
    use std::ffi::CString;
    let _ = std::fs::remove_file(path);
    let c = CString::new(path).context("a FIFO path with a nul byte in it")?;
    // 0o600: the plugin runs as the core does, and nothing else on the machine
    // has any business reading the programme.
    let made = unsafe { libc::mkfifo(c.as_ptr(), 0o600) };
    anyhow::ensure!(made == 0, "could not make the FIFO at {path}: {}", std::io::Error::last_os_error());
    Ok(())
}

#[cfg(not(unix))]
fn make_fifo(_path: &str) -> Result<()> {
    anyhow::bail!("this platform has no FIFO; a sidecar output needs one")
}

impl Output for SidecarOutput {
    fn manifest(&self) -> &Manifest {
        &self.spec.manifest
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.id = hello.instance.clone();
        self.spec.canvas = hello.canvas.clone();
        self.handshake(&hello.instance, &hello.params)?;
        Ok(Ready {
            manifest: self.spec.manifest,
            latency_ms: self.spec.manifest.latency_ms,
            capabilities: self.spec.manifest.capabilities,
        })
    }

    fn build(
        &mut self,
        ctx: &OutputCtx<'_>,
        video: &gst::Element,
        audio: &gst::Element,
    ) -> Result<()> {
        let id = ctx.id;
        let gen = ctx.generation;
        let fifo = self
            .media
            .as_ref()
            .context("the plugin has no media address; the handshake has not run")?
            .programme();
        // Streamable Matroska: one container that carries H.264 and AAC as
        // happily as it carries raw I420 and F32LE, which is why the browser
        // sidecar already writes it in the other direction.
        let mux = make("matroskamux", &format!("{id}-plugin-mux-{gen}"))?;
        mux.set_property("streamable", true);
        let sink = make("filesink", &format!("{id}-plugin-sink-{gen}"))?;
        sink.set_property("location", &fifo);
        // A FIFO is not a file: seeking it fails and buffering it adds latency
        // nobody asked for.
        crate::probe::set_bool(&sink, "buffer-mode", false);
        crate::probe::set_bool(&sink, "async", false);
        crate::probe::set_bool(&sink, "sync", false);
        ctx.pipeline.add_many([&mux, &sink]).context("adding the plugin output")?;
        gst::Element::link(&mux, &sink).context("linking the plugin muxer to its FIFO")?;
        video.link(&mux).context("linking the programme video to the plugin")?;
        if self.spec.manifest.media.audio != StreamMode::None {
            audio.link(&mux).context("linking the programme audio to the plugin")?;
        }
        for element in [&mux, &sink] {
            element.sync_state_with_parent().ok();
        }
        let child = self.child.as_ref().context(
            "the plugin is not running, so there is nowhere to send the programme. Call \
             plugin.reload.",
        )?;
        child.call(
            "start",
            json!({
                "canvas": canvas_of(&self.spec.canvas),
                "transport": "container",
                "media": fifo,
            }),
        )?;
        self.started = true;
        Ok(())
    }

    fn connected(&self) -> bool {
        // What the plugin says, not what the pipe says: a sink that accepts
        // buffers says nothing about whether the far end answered.
        self.started
            && matches!(self.instance_state(), InstanceState::Running | InstanceState::Ready)
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        let child = self.child.as_ref().context("the plugin is not running")?;
        let answer = child.call("configure", json!({ "params": params_json(params) }))?;
        if answer.get("applied").and_then(Value::as_bool).unwrap_or(false) {
            return Ok(Configure::Applied);
        }
        Ok(Configure::RestartRequired(
            answer
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("the plugin did not say why")
                .to_string(),
        ))
    }

    fn health(&self) -> Health {
        let Some(child) = self.child.as_ref() else {
            return Health::of(PluginState::Stopped);
        };
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
                debug!(output = %self.id, ?e, "the plugin did not answer `health`");
                Health {
                    state: PluginState::Degraded,
                    detail: Some("it did not answer `health` in time".into()),
                }
            }
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            "keyframe" | "tool.call" => {
                let child = self.child.as_ref().context("the plugin is not running")?;
                child.call(method, params)
            }
            other => Err(unknown_method(&self.spec.manifest, other, &["keyframe", "tool.call"])),
        }
    }
}

impl Drop for SidecarOutput {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            child.shutdown("the output was removed");
        }
    }
}
