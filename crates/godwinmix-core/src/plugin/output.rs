//! The `Output` trait and the table that picks an implementation.
//!
//! An output consumes the encoded programme and sends or stores it. Everything
//! generic about one (the feed queues on the programme side, the proxy pair,
//! the reconnect backoff, the overflow watchdog, the keyframe request) lives in
//! `output.rs`. What is left, and what this trait is, is the muxer, the sink
//! and the answer to "are we actually connected".
//!
//! That last one used to be an RTMP only reading of `stats.out-chunk-size`.
//! Liveness is a trait method now, so an output that has no chunk size can
//! still tell the truth about itself.

use super::{Configure, Health, Hello, Manifest, Ready};
use crate::config::{OutputConfig, Params};
use anyhow::Result;
use gstreamer as gst;
use serde_json::Value;

/// What an output is given when it builds its half of the pipeline.
pub struct OutputCtx<'a> {
    pub id: &'a str,
    /// Bumped on every reconnect, so element names stay unique across
    /// generations of the same output.
    pub generation: u32,
    pub pipeline: &'a gst::Pipeline,
    pub params: &'a Params,
    pub cfg: &'a OutputConfig,
}

pub trait Output: Send {
    fn manifest(&self) -> &Manifest;

    fn initialize(&mut self, hello: Hello) -> Result<Ready>;

    /// Build the muxer and the sink into `ctx.pipeline` and link the two
    /// queues into them. Everything upstream of `video` and `audio` is the
    /// core's, and is the same whatever the destination is.
    fn build(
        &mut self,
        ctx: &OutputCtx<'_>,
        video: &gst::Element,
        audio: &gst::Element,
    ) -> Result<()>;

    /// Whether the far end has actually accepted us. Read from the sink rather
    /// than inferred from data flowing: a sink that connects in the background
    /// accepts buffers straight away and says nothing about whether anyone
    /// answered.
    fn connected(&self) -> bool;

    fn configure(&mut self, params: &Params) -> Result<Configure>;

    fn health(&self) -> Health;

    fn call(&mut self, method: &str, params: Value) -> Result<Value>;
}

/// One entry in the output registry.
pub struct OutputProvide {
    pub manifest: Manifest,
    pub claims: fn(&str) -> Option<u16>,
    pub make: fn(&OutputConfig) -> Result<Box<dyn Output>>,
}

static REGISTRY: &[OutputProvide] = &[super::outputs::rtmp::PROVIDE, super::outputs::srt::PROVIDE];

pub fn registry() -> &'static [OutputProvide] {
    REGISTRY
}

pub fn by_type(type_id: &str) -> Option<&'static OutputProvide> {
    registry().iter().find(|p| p.manifest.is(type_id))
}

pub fn resolve(uri: &str) -> Option<&'static OutputProvide> {
    registry()
        .iter()
        .filter_map(|p| (p.claims)(uri).map(|rank| (rank, p)))
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, p)| p)
}

pub fn available() -> Vec<String> {
    registry().iter().map(|p| p.manifest.provide_id()).collect()
}

/// Every output kind this build carries, with what it is and what it claims.
pub fn described() -> Vec<super::KindInfo> {
    registry().iter().map(|p| p.manifest.describe()).collect()
}

/// The output implementation a config entry names, whether it wrote a `type`
/// or only a URI.
pub fn resolve_config(cfg: &OutputConfig) -> Result<&'static OutputProvide> {
    if let Some(t) = cfg.type_id.as_deref().filter(|t| !t.trim().is_empty()) {
        return by_type(t.trim()).ok_or_else(|| {
            anyhow::anyhow!(
                "no output type `{t}` in this build. It has: {}",
                available().join(", ")
            )
        });
    }
    resolve(&cfg.uri).ok_or_else(|| {
        anyhow::anyhow!(
            "nothing in this build sends to `{}`. Write `type` to say what it is; this build has: {}",
            cfg.uri,
            available().join(", ")
        )
    })
}

/// Make the output this config names, and run its handshake.
pub fn open(cfg: &OutputConfig) -> Result<(Box<dyn Output>, Ready)> {
    let provide = resolve_config(cfg)?;
    let mut out = (provide.make)(cfg)?;
    let ready = out.initialize(Hello {
        instance: cfg.id.clone(),
        canvas: crate::caps::CanvasCaps::new(&crate::config::Canvas::default()),
        api_level: super::API_LEVEL,
        params: cfg.effective_params(),
        tier: super::Tier::Core,
    })?;
    Ok((out, ready))
}

/// Link a queue to a muxer's request pad, asking for a named template first
/// and falling back to whatever the muxer offers.
///
/// `flvmux` names its pads `video` and `audio`; `mpegtsmux` calls both
/// `sink_%d`. One helper covers both, so an output implementation says what it
/// wants and not how to ask for it.
pub fn link_to_mux(
    queue: &gst::Element,
    mux: &gst::Element,
    templates: &[&str],
) -> Result<gst::Pad> {
    use anyhow::Context;
    use gstreamer::prelude::*;
    let pad = templates
        .iter()
        .find_map(|t| mux.request_pad_simple(t))
        .with_context(|| {
            format!("{} refused a pad; tried {}", mux.name(), templates.join(", "))
        })?;
    let src = queue.static_pad("src").context("queue has no src pad")?;
    src.link(&pad).with_context(|| format!("linking {} into {}", queue.name(), mux.name()))?;
    Ok(pad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_uri_picks_the_output_that_speaks_it() {
        for (uri, want) in [
            ("rtmp://host/app/key", "rtmp/output"),
            ("rtmps://host/app/key", "rtmp/output"),
            ("srt://host:9000", "srt/output"),
            ("srt://host:9000?mode=caller", "srt/output"),
        ] {
            let got = resolve(uri).unwrap_or_else(|| panic!("nothing claimed {uri}"));
            assert_eq!(got.manifest.provide_id(), want, "for {uri}");
        }
    }

    #[test]
    fn an_unknown_destination_names_what_the_build_can_send_to() {
        let cfg = OutputConfig::bare("x", "whip://example.com/ingest");
        let err = match resolve_config(&cfg) {
            Ok(p) => panic!("this build should not send to whip, but {} claimed it", p.manifest.provide_id()),
            Err(e) => e,
        };
        let text = format!("{err}");
        assert!(text.contains("rtmp/output"), "{text}");
        assert!(text.contains("srt/output"), "{text}");
    }
}
