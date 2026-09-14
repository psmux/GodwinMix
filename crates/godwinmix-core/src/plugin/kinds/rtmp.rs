//! `rtmp/source`: an RTMP or RTMPS feed, demuxed and decoded explicitly.
//!
//! The one kind that names its own client element, because two implementations
//! exist and neither talks to every server. When a feed connects and then
//! delivers nothing, the supervisor asks this kind to swap the client once.

use super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::{self, make};
use crate::input::make_rtmp_source;
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds,
    PluginState, ProvideKind, Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::warn;

pub const MANIFEST: Manifest = Manifest {
    plugin: "rtmp",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "An RTMP or RTMPS publisher, decoded with the hardware the machine has",
    uri_schemes: &["rtmp://", "rtmps://"],
    rank: 240,
    media: MediaDecl {
        video: StreamMode::Container,
        audio: StreamMode::Container,
        alpha: false,
        thumb: true,
    },
    capabilities: CapabilitySet::new()
        .with(Capability::RestartInPlace)
        .with(Capability::Health),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    let lower = uri.trim().to_lowercase();
    (lower.starts_with("rtmp://") || lower.starts_with("rtmps://")).then_some(MANIFEST.rank)
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    Ok(Box::new(RtmpSource {
        ctx: req.ctx(),
        pipeline: None,
        src: Mutex::new(None),
        src_queue: None,
        fallback_used: AtomicBool::new(false),
    }))
}

pub struct RtmpSource {
    ctx: BuildCtx,
    pipeline: Option<gst::Pipeline>,
    /// The client element, swappable once. Behind a lock because the
    /// supervisor asks for the swap from its own thread.
    src: Mutex<Option<gst::Element>>,
    src_queue: Option<gst::Element>,
    fallback_used: AtomicBool,
}

impl Source for RtmpSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.ctx.canvas = hello.canvas;
        Ok(Ready {
            manifest: MANIFEST,
            latency_ms: MANIFEST.latency_ms,
            capabilities: MANIFEST.capabilities,
        })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let ctx = &self.ctx;
        let id = &ctx.id;
        let src = make_rtmp_source(ctx.cfg.rtmp_client.first_element(), id, &ctx.cfg.uri)?;
        // A short ingest queue decouples the network thread from decoding.
        let src_queue = gstutil::queue_time(&format!("{id}-ingest-q"), 2.0, false)?;
        let demux = make("flvdemux", &format!("{id}-demux"))?;
        let h264parse = make("h264parse", &format!("{id}-h264parse"))?;
        let decoder = make(ctx.backends.video_decode.element, &format!("{id}-vdec"))?;
        // A hardware decoder may hand out frames in GPU memory; the download
        // element brings them back. Absent is not fatal, caps negotiation
        // usually inserts one.
        let download = match ctx.backends.video_decode.download {
            Some(f) if crate::probe::exists(f) => Some(make(f, &format!("{id}-vdl"))?),
            Some(f) => {
                tracing::debug!(source = %id, element = f, "download element absent, relying on caps negotiation");
                None
            }
            None => None,
        };
        let aacparse = make("aacparse", &format!("{id}-aacparse"))?;
        let adec = make(ctx.backends.audio_decode, &format!("{id}-adec"))?;

        let mut els = vec![
            src.clone(),
            src_queue.clone(),
            demux.clone(),
            h264parse.clone(),
            decoder.clone(),
            aacparse.clone(),
            adec.clone(),
        ];
        els.extend(download.clone());

        let ends = assemble(
            ctx,
            thumb,
            Ingest::default().with(els).livesync(true),
            |w: &Wiring| {
                gst::Element::link_many([&src, &src_queue, &demux]).context("linking ingest")?;
                let mut vchain = vec![&h264parse, &decoder];
                if let Some(d) = &download {
                    vchain.push(d);
                }
                let entry = w.norm.video_entry();
                vchain.push(&entry);
                gst::Element::link_many(&vchain).context("linking the rtmp video head")?;
                gst::Element::link_many([&aacparse, &adec, &w.norm.audio_entry()])
                    .context("linking the rtmp audio head")?;
                w.route(&demux, h264parse.clone(), aacparse.clone());
                Ok(KindParts::default())
            },
        )?;
        self.pipeline = Some(ends.pipeline.clone());
        *self.src.lock() = Some(src);
        self.src_queue = Some(src_queue);
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        validate(params)?;
        Ok(Configure::RestartRequired(
            "an rtmp source takes a new address or client by being built again".into(),
        ))
    }

    fn health(&self) -> Health {
        Health::of(if self.pipeline.is_some() {
            PluginState::Running
        } else {
            PluginState::Starting
        })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            // Nothing outside the pipeline to bring back: the core NULLs it and
            // starts it again.
            "restart" => Ok(Value::Null),
            "client.fallback" => Ok(json!({ "swapped": self.swap_client()? })),
            other => Err(unknown_method(&MANIFEST, other, &["restart", "client.fallback"])),
        }
    }
}

impl RtmpSource {
    /// Swap in the other RTMP client implementation, once.
    ///
    /// Returns false when the configuration pins a client, or when the swap has
    /// already been used. Called only for a source that has produced no media
    /// at all, so nothing downstream has state to lose.
    fn swap_client(&self) -> Result<bool> {
        let (Some(pipeline), Some(queue)) = (&self.pipeline, &self.src_queue) else {
            return Ok(false);
        };
        let Some(element) = self.ctx.cfg.rtmp_client.fallback_element() else {
            return Ok(false);
        };
        if self.fallback_used.swap(true, Ordering::SeqCst) {
            return Ok(false);
        }
        warn!(
            source = %self.ctx.id,
            from = self.ctx.cfg.rtmp_client.first_element(),
            to = element,
            "no media arrived from this RTMP client, trying the other implementation"
        );
        pipeline.set_state(gst::State::Null).ok();
        let fresh = make_rtmp_source(element, &self.ctx.id, &self.ctx.cfg.uri)?;
        let mut current = self.src.lock();
        if let Some(old) = current.as_ref() {
            pipeline.remove(old).context("removing the old rtmp source")?;
        }
        pipeline.add(&fresh).context("adding the replacement rtmp source")?;
        fresh.link(queue).context("linking the replacement rtmp source")?;
        *current = Some(fresh);
        Ok(true)
    }
}

/// What `params` may carry on an rtmp source, and what a bad one is told.
pub fn validate(params: &Params) -> Result<()> {
    for (key, value) in params {
        match key.as_str() {
            "uri" => {
                anyhow::ensure!(value.is_str(), "rtmp/source params.uri must be a string");
            }
            "client" => {
                let s = value.as_str().unwrap_or_default();
                anyhow::ensure!(
                    matches!(s, "auto" | "rtmp2src" | "rtmpsrc"),
                    "rtmp/source params.client must be auto, rtmp2src or rtmpsrc, not `{s}`"
                );
            }
            _ => {}
        }
    }
    Ok(())
}
