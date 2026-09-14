//! `SidecarFilter`: raw in, raw out, across a process boundary.
//!
//! A filter sits between two elements that both speak the canvas contract, so
//! a sidecar one is a bin with a `sink` and a `src` ghost pad and a round trip
//! through another process in the middle. The transport is the same one a
//! source uses and it is used twice, once each way.
//!
//! On the container transport that is a pipe out and a pipe back with a
//! Matroska stream on each, which costs a mux and a demux per frame and is
//! honest about it: the harness prints the added latency and the author sees
//! what the choice costs before publishing. On `unixfd` and `shm` it is two
//! sockets and no copy at all.
//!
//! A filter that cannot be started is not a filter the programme waits for.
//! The bin passes the media through untouched and says so in the log, because
//! a chroma key that fails to load must not take the picture with it.

use super::process::Sidecar;
use super::source::{canvas_of, params_json, SidecarSpec};
use super::transport::{self, MediaDir};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::plugin::filter::{Filter, Stream};
use crate::plugin::{Configure, Manifest};
use anyhow::{Context, Result};
use godwinmix_protocol::plugin::wire::Transport;
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};
use tracing::warn;

pub struct SidecarFilter {
    spec: SidecarSpec,
    child: Option<Sidecar>,
    media: Option<MediaDir>,
    id: String,
    latency_ms: u32,
}

impl SidecarFilter {
    pub fn new(spec: SidecarSpec, id: String) -> Self {
        let latency_ms = spec.manifest.latency_ms;
        Self { spec, child: None, media: None, id, latency_ms }
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(Sidecar::pid)
    }

    /// Spawn, shake hands, and settle a transport that can go both ways.
    fn start(&mut self, canvas: &CanvasCaps, params: &Params) -> Result<Transport> {
        let instance = self.id.clone();
        let runtime = self.spec.runtime.clone();
        let mut child = Sidecar::spawn(&instance, &self.spec.launch)
            .with_context(|| format!("starting `{}`", self.spec.launch.command_line()))?;
        let mut made: Option<MediaDir> = None;
        let outcome = child.handshake(
            Some(&self.spec.plugin),
            canvas_of(canvas),
            &self.spec.provide,
            params_json(params),
            |t| {
                transport::usable(t)?;
                let dir = MediaDir::create(&runtime, &instance)?;
                let base = dir.base();
                made = Some(dir);
                Ok(base)
            },
        );
        if let Err(e) = outcome {
            child.lifecycle_mut().failed(e.to_string());
            child.shutdown("the handshake failed");
            return Err(e);
        }
        let transport = child.transport().unwrap_or(Transport::Container);
        let answer = child.call(
            "start",
            json!({
                "canvas": canvas_of(canvas),
                "transport": transport,
                "media": child.media_address(),
            }),
        )?;
        if let Some(ms) = answer.get("latency_ms").and_then(Value::as_u64) {
            self.latency_ms = ms as u32;
        }
        self.media = made;
        self.child = Some(child);
        Ok(transport)
    }

    /// The bin that carries a frame out to the plugin and back.
    fn round_trip(&self, transport: Transport, canvas: &CanvasCaps) -> Result<gst::Element> {
        let id = &self.id;
        let media = self.media.as_ref().context("the filter has no media address")?;
        anyhow::ensure!(
            transport != Transport::Container,
            "a container round trip for a filter is not built yet: it would mux and demux \
             every frame twice and the cost is not worth shipping untested. Declare 'unixfd' \
             or 'shm' in the provide's transports, or write the filter as a tier 0 or tier 1 \
             filter, which is what chroma/filter is."
        );
        let bin = gst::Bin::with_name(&format!("{id}-sidecar-filter"));
        let (sink_factory, src_factory) = match transport {
            Transport::Unixfd => ("unixfdsink", "unixfdsrc"),
            Transport::Shm => ("shmsink", "shmsrc"),
            Transport::Container => unreachable!("refused above"),
        };
        // Out to the plugin on one socket, back from it on another.
        let out = make(sink_factory, &format!("{id}-filter-out"))?;
        out.set_property("socket-path", media.video());
        let back = make(src_factory, &format!("{id}-filter-back"))?;
        back.set_property("socket-path", media.audio());
        crate::probe::set_bool(&back, "is-live", true);
        let caps = make("capsfilter", &format!("{id}-filter-caps"))?;
        caps.set_property("caps", canvas.video());
        bin.add_many([&out, &back, &caps]).context("adding the filter's ends")?;
        gst::Element::link(&back, &caps).context("linking the filter's return")?;
        let sink_pad = out.static_pad("sink").context("the filter sink has no pad")?;
        let src_pad = caps.static_pad("src").context("the filter capsfilter has no pad")?;
        bin.add_pad(&gst::GhostPad::with_target(&sink_pad)?).context("the filter's sink pad")?;
        bin.add_pad(&gst::GhostPad::with_target(&src_pad)?).context("the filter's src pad")?;
        Ok(bin.upcast())
    }

    /// A bin that does nothing, for a filter that could not be started.
    fn passthrough(&self) -> Result<gst::Element> {
        let bin = gst::Bin::with_name(&format!("{}-sidecar-bypass", self.id));
        let queue = make("identity", &format!("{}-bypass", self.id))?;
        bin.add(&queue).context("adding the bypass")?;
        let sink = queue.static_pad("sink").context("no sink pad on identity")?;
        let src = queue.static_pad("src").context("no src pad on identity")?;
        bin.add_pad(&gst::GhostPad::with_target(&sink)?).context("the bypass sink pad")?;
        bin.add_pad(&gst::GhostPad::with_target(&src)?).context("the bypass src pad")?;
        Ok(bin.upcast())
    }
}

impl Filter for SidecarFilter {
    fn manifest(&self) -> &Manifest {
        &self.spec.manifest
    }

    fn build(&mut self, canvas: &CanvasCaps, params: &Params) -> Result<gst::Element> {
        match self.start(canvas, params).and_then(|t| self.round_trip(t, canvas)) {
            Ok(bin) => Ok(bin),
            Err(e) => {
                // The programme never waits for a plugin. A filter that cannot
                // start passes the picture through and the operator is told.
                warn!(
                    filter = %self.id,
                    plugin = %self.spec.plugin.plugin.name,
                    "{e:#}; the filter is bypassed and the picture is untouched"
                );
                self.passthrough()
            }
        }
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        let Some(child) = self.child.as_ref() else {
            return Ok(Configure::RestartRequired(
                "the filter is bypassed because its plugin would not start".into(),
            ));
        };
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

    fn latency_ms(&self) -> u32 {
        self.latency_ms
    }

    fn stream(&self) -> Stream {
        if self.spec.manifest.media.video.present() {
            Stream::Video
        } else {
            Stream::Audio
        }
    }
}

impl Drop for SidecarFilter {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            child.shutdown("the filter was removed");
        }
    }
}
