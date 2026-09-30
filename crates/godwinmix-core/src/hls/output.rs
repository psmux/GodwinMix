//! `hls/output`: the programme as HLS and LL-HLS, served from the control
//! port.
//!
//! Registered with the other built in outputs, so it gets what they get: its
//! own pipeline behind the proxy pair, the feed queue that rides out a
//! rebuild, and the encoded programme on two queues. With no ladder it
//! packages that encode as it is, one rung and the audio, which costs a
//! parser and a muxer. With a ladder preset it decodes the programme once and
//! encodes each rung itself; that is the stand in until the rendition
//! planner hands [`super::attach`] its own encoders (see `mod.rs`).

use super::ladder::{self, Rung};
use super::stream::{self, Stream};
use super::track::TrackKind;
use super::{attach, HlsParams, Input};
use crate::config::{OutputConfig, Params};
use crate::gstutil::make;
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
use tracing::warn;

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
    Ok(Box::new(HlsOutput { id: cfg.id.clone(), params: HlsParams::default(), ladder: None, stream: None }))
}

pub struct HlsOutput {
    id: String,
    params: HlsParams,
    ladder: Option<Vec<Rung>>,
    stream: Option<Arc<Stream>>,
}

/// The ladder an output's params ask for: `ladder = "abr-ladder-4"`, or the
/// same name as `rendition = { preset = "abr-ladder-4" }`.
pub fn ladder_of(params: &Params) -> Result<Option<Vec<Rung>>> {
    let named = params
        .get("ladder")
        .and_then(|v| v.as_str())
        .or_else(|| params.get("rendition").and_then(|r| r.get("preset")).and_then(|v| v.as_str()));
    let Some(name) = named else { return Ok(None) };
    ladder::preset(name).map(Some).with_context(|| {
        format!("hls/output has no ladder called `{name}`. It has abr-ladder-4 (1080p, 720p, 480p, 360p) and abr-ladder-3 (720p, 480p, 360p); leave it out to serve the programme as it is.")
    })
}

fn check_elements(ladder: bool) -> Result<()> {
    let mut need = vec!["cmafmux", "appsink"];
    if ladder {
        need.extend(["decodebin", "x264enc", "videoscale"]);
    }
    let missing: Vec<&str> = need.into_iter().filter(|e| !crate::probe::exists(e)).collect();
    anyhow::ensure!(
        missing.is_empty(),
        "serving HLS needs the GStreamer elements {}. cmafmux is in gst-plugins-rs (gstreamer1.0-plugins-rs \
         or the `fmp4` plugin), x264enc in gst-plugins-ugly.",
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
        self.ladder = ladder_of(&hello.params)?;
        check_elements(self.ladder.is_some())?;
        Ok(Ready { manifest: MANIFEST, latency_ms: self.params.segment_ms, capabilities: MANIFEST.capabilities })
    }

    fn build(&mut self, ctx: &OutputCtx<'_>, video: &gst::Element, audio: &gst::Element) -> Result<()> {
        let stream = self
            .stream
            .get_or_insert_with(|| {
                let s = Arc::new(Stream::new(&self.id, self.params));
                stream::publish(s.clone());
                s
            })
            .clone();
        let pad = |el: &gst::Element| el.static_pad("src").context("the output queue has no src pad");
        attach(ctx.pipeline, &stream, Input { id: "audio", kind: TrackKind::Audio, pad: &pad(audio)?, declared_kbps: 0 })?;
        match self.ladder.clone() {
            None => {
                let input = Input { id: "programme", kind: TrackKind::Video, pad: &pad(video)?, declared_kbps: 0 };
                attach(ctx.pipeline, &stream, input)?;
            }
            Some(rungs) => transcode(ctx.pipeline, &stream, &pad(video)?, rungs, ctx.generation)?,
        }
        Ok(())
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

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        HlsParams::from_params(params)?;
        ladder_of(params)?;
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
        if let Some(s) = self.stream.take() {
            stream::withdraw(&s);
        }
    }
}

/// Decode the programme once and hand the raw picture to a ladder, whose
/// rungs are packaged as they appear.
fn transcode(pipeline: &gst::Pipeline, stream: &Arc<Stream>, video: &gst::Pad, rungs: Vec<Rung>, gen: u32) -> Result<()> {
    let tag = format!("hls-{}-ladder-{gen}", stream.id);
    let decode = make("decodebin", &format!("{tag}-decode"))?;
    pipeline.add(&decode).context("adding the ladder decoder")?;
    let (weak, stream) = (pipeline.downgrade(), stream.clone());
    let segment_ms = stream.params.segment_ms;
    decode.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        let raw = pad.current_caps().and_then(|c| c.structure(0).map(|s| s.name().starts_with("video/x-raw")));
        if raw == Some(false) {
            return;
        }
        let built = ladder::encode(&pipeline, pad, &rungs, segment_ms, &tag).and_then(|pads| {
            for (rung, enc) in pads {
                let input = Input { id: &rung.id, kind: TrackKind::Video, pad: &enc, declared_kbps: rung.kbps };
                attach(&pipeline, &stream, input)?;
            }
            Ok(())
        });
        if let Err(e) = built {
            warn!(error = %e, output = %stream.id, "the HLS ladder could not be built");
        }
    });
    decode.sync_state_with_parent().ok();
    video.link(&decode.static_pad("sink").context("decodebin has no sink pad")?).context("linking the programme into the ladder decoder")?;
    Ok(())
}
