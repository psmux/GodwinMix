//! `whep/output`: the programme, or a rendition of it, to WebRTC viewers.
//!
//! Registered with the other built in outputs, so it gets what they get: its
//! own pipeline behind the proxy pair, a lease on the programme encoder while
//! it exists, and a rendition when it asks for one. The video is never
//! encoded here: it is parsed and put on a tee each viewer's payloader reads.
//! The sound is made Opus once, because WebRTC has no AAC, and shared by
//! every viewer.
//!
//! ```text
//!   video ──► parser ──► tee ──► one branch per viewer (session.rs)
//!   audio ──► decodebin ──► convert ──► resample ──► opusenc ──► tee ──► ...
//! ```

use super::params::{video_send, WhepParams};
use super::server::Server;
use super::tees::{opus_branch, tee};
use crate::config::{OutputConfig, Params};
use crate::gstutil::make;
use crate::plugin::output::{Output, OutputCtx, OutputProvide};
use crate::plugin::source::unknown_method;
use crate::plugin::{
    CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, PluginState, ProvideKind, Ready, StreamMode, Tier,
    API_LEVEL,
};
use anyhow::{Context, Result};
use godwinmix_protocol::rendition::VideoCodec;
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};
use std::sync::Arc;

pub const MANIFEST: Manifest = Manifest {
    plugin: "whep",
    id: "output",
    kind: ProvideKind::Output,
    api: API_LEVEL,
    description: "WebRTC playback of the programme or a rendition, served from the control port as WHEP",
    uri_schemes: &[],
    rank: 200,
    media: MediaDecl { video: StreamMode::Container, audio: StreamMode::Container, alpha: false, thumb: false },
    capabilities: CapabilitySet::new(),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: OutputProvide = OutputProvide { manifest: MANIFEST, claims: |_| None, make: new };

/// The elements every viewer needs, checked when the output is added.
const NEEDED: &[&str] = &["webrtcbin", "nicesrc", "rtph264pay", "rtpopuspay", "opusenc", "decodebin"];

fn new(cfg: &OutputConfig) -> Result<Box<dyn Output>> {
    Ok(Box::new(WhepOutput { id: cfg.id.clone(), params: WhepParams::default(), key: String::new(), server: None }))
}

pub struct WhepOutput {
    id: String,
    params: WhepParams,
    /// The viewer key: params.viewer_key, or derived from the id.
    key: String,
    server: Option<Arc<Server>>,
}

impl Output for WhepOutput {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.params = WhepParams::from_params(&hello.params)?;
        self.key = match &self.params.viewer_key {
            Some(k) => k.clone(),
            None => crate::hls::key::viewer_key(&self.id)?,
        };
        let missing = crate::setup::system::absent(NEEDED);
        if !missing.is_empty() {
            let what = "Sending the programme to browsers over WebRTC";
            return Err(crate::setup::system::missing_error(what, crate::setup::system::WEBRTC, &missing));
        }
        Ok(Ready { manifest: MANIFEST, latency_ms: 0, capabilities: MANIFEST.capabilities })
    }

    fn build(&mut self, ctx: &OutputCtx<'_>, video: &gst::Element, audio: &gst::Element) -> Result<()> {
        let codec = ctx.taps.first().and_then(|t| t.video).map(|v| v.codec).unwrap_or(VideoCodec::H264);
        let send = video_send(codec).with_context(|| {
            format!("whep/output cannot send {codec:?} over WebRTC. Ask its rendition for H.264, VP8, VP9, AV1 or H.265.")
        })?;
        let (id, gen) = (ctx.id, ctx.generation);
        let vtee = tee(&format!("whep-{id}-vtee-{gen}"))?;
        let mut chain = vec![video.clone()];
        if let Some(parser) = send.parser {
            let p = make(parser, &format!("whep-{id}-vparse-{gen}"))?;
            crate::probe::set_int(&p, "config-interval", -1);
            ctx.pipeline.add(&p).context("adding the WHEP parser")?;
            chain.push(p);
        }
        ctx.pipeline.add(&vtee).context("adding the WHEP video tee")?;
        chain.push(vtee.clone());
        gst::Element::link_many(&chain).context("linking the programme video to the WHEP tee")?;
        let atee = opus_branch(ctx.pipeline, audio, &format!("whep-{id}-{gen}"))?;
        let server = self.server.get_or_insert_with(|| Server::new(&self.id, self.params.clone(), self.key.clone())).clone();
        server.attach(ctx.pipeline, &vtee, Some(&atee), send);
        super::publish(server);
        Ok(())
    }

    fn connected(&self) -> bool {
        self.server.as_ref().is_some_and(|s| s.ready())
    }

    fn status(&self) -> godwinmix_protocol::types::Extra {
        let mut out = godwinmix_protocol::types::Extra::new();
        out.insert("type".into(), json!("whep/output"));
        out.insert("whep_path".into(), json!(format!("/whep/{}?key={}", self.id, self.key)));
        out.insert("viewers".into(), json!(self.server.as_ref().map_or(0, |s| s.viewers())));
        out.insert("max_viewers".into(), json!(self.params.max_viewers));
        out
    }

    fn shutdown(&mut self, pipeline: gst::Pipeline) {
        if let Some(s) = &self.server {
            s.detach();
        }
        let _ = pipeline.set_state(gst::State::Null);
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        WhepParams::from_params(params)?;
        Ok(Configure::RestartRequired("a whep output takes new ICE servers or a new viewer limit by being added again".into()))
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

impl Drop for WhepOutput {
    fn drop(&mut self) {
        if let Some(s) = self.server.take() {
            s.detach();
            super::withdraw(&s);
        }
    }
}
