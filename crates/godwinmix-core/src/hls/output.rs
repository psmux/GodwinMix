//! `hls/output`: the programme as HLS and LL-HLS, served from the control
//! port.
//!
//! Registered with the other built in outputs, so it gets what they get: its
//! own pipeline behind the proxy pair, the feed queue that rides out a
//! rebuild, and the encoded programme on two queues. With no rendition it
//! packages that encode as it is, one rung and the audio, which costs a
//! parser and a muxer. With a ladder it encodes nothing itself: the mixer
//! plans its rungs with every other output's renditions ([`super::request`]
//! says what it asks for), and [`super::rungs`] packages what the plan made.

use super::rungs::Rungs;
use super::stream::{self, Stream};
use super::track::TrackKind;
use super::{attach, HlsParams, Input};
use crate::config::{OutputConfig, Params};
use crate::plugin::output::{Output, OutputCtx, OutputProvide};
use crate::plugin::source::unknown_method;
use crate::plugin::{
    CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, PluginState, ProvideKind, Ready,
    StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};
use std::sync::Arc;

pub const MANIFEST: Manifest = Manifest {
    plugin: "hls",
    id: "output",
    kind: ProvideKind::Output,
    api: API_LEVEL,
    description: "HLS and LL-HLS served from the control port, one rung or an ABR ladder",
    uri_schemes: &[],
    rank: 200,
    media: MediaDecl {
        video: StreamMode::Container,
        audio: StreamMode::Container,
        alpha: false,
        thumb: false,
    },
    capabilities: CapabilitySet::new(),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: OutputProvide = OutputProvide { manifest: MANIFEST, claims: |_| None, make: new };

fn new(cfg: &OutputConfig) -> Result<Box<dyn Output>> {
    Ok(Box::new(HlsOutput {
        id: cfg.id.clone(),
        params: HlsParams::default(),
        viewer_key: String::new(),
        stream: None,
        rungs: Rungs::default(),
    }))
}

pub struct HlsOutput {
    id: String,
    /// `params.viewer_key`, or the one this machine derives for the id.
    viewer_key: String,
    params: HlsParams,
    stream: Option<Arc<Stream>>,
    /// This generation's branches off the lower rungs' tees.
    rungs: Rungs,
}

fn check_elements() -> Result<()> {
    let missing: Vec<&str> = ["cmafmux", "appsink"].into_iter().filter(|e| !crate::probe::exists(e)).collect();
    anyhow::ensure!(
        missing.is_empty(),
        "serving HLS needs the GStreamer elements {}. cmafmux is in gst-plugins-rs (gstreamer1.0-plugins-rs \
         or the `fmp4` plugin).",
        missing.join(", ")
    );
    Ok(())
}

impl Output for HlsOutput {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.params = HlsParams::from_params(&hello.params)?;
        self.viewer_key = match hello.params.get("viewer_key").and_then(|v| v.as_str()) {
            Some(k) if k.len() >= 16 => k.to_string(),
            Some(_) => anyhow::bail!("hls/output params.viewer_key must be at least 16 characters, or left out for the one this machine makes"),
            None => super::key::viewer_key(&self.id)?,
        };
        check_elements()?;
        Ok(Ready { manifest: MANIFEST, latency_ms: self.params.segment_ms, capabilities: MANIFEST.capabilities })
    }

    fn build(&mut self, ctx: &OutputCtx<'_>, video: &gst::Element, audio: &gst::Element) -> Result<()> {
        let stream = self
            .stream
            .get_or_insert_with(|| {
                let s = Arc::new(Stream::new(&self.id, self.params, &self.viewer_key));
                stream::publish(s.clone());
                s
            })
            .clone();
        let pad = |el: &gst::Element| el.static_pad("src").context("the output queue has no src pad");
        attach(ctx.pipeline, &stream, Input { id: "audio", kind: TrackKind::Audio, pad: &pad(audio)?, declared_kbps: 0 })?;
        if ctx.taps.is_empty() {
            let input = Input { id: "programme", kind: TrackKind::Video, pad: &pad(video)?, declared_kbps: 0 };
            attach(ctx.pipeline, &stream, input)?;
            return Ok(());
        }
        self.rungs.build(ctx, &stream, &pad(video)?)
    }

    fn connected(&self) -> bool {
        self.stream.as_ref().is_some_and(|s| s.ready())
    }

    fn status(&self) -> godwinmix_protocol::types::Extra {
        let mut out = godwinmix_protocol::types::Extra::new();
        out.insert("type".into(), json!("hls/output"));
        if let Some(Value::Object(map)) = self.stream.as_ref().map(|s| s.status()) {
            out.extend(map);
        }
        out
    }

    fn shutdown(&mut self, pipeline: gst::Pipeline) {
        let _ = pipeline.set_state(gst::State::Null);
        self.rungs.detach();
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        HlsParams::from_params(params)?;
        Ok(Configure::RestartRequired("an hls output takes new segment lengths or a new ladder by being added again".into()))
    }

    fn health(&self) -> Health {
        Health::of(if self.connected() { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, _: Value) -> Result<Value> {
        match method {
            "stats" => Ok(json!(self.status())),
            other => Err(unknown_method(&MANIFEST, other, &["stats"])),
        }
    }
}

impl Drop for HlsOutput {
    fn drop(&mut self) {
        self.rungs.detach();
        if let Some(s) = self.stream.take() {
            stream::withdraw(&s);
        }
    }
}
