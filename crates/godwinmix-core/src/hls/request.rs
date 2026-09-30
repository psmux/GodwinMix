//! What an `hls/output` asks the rendition planner for: the programme as it
//! is, a named ladder, or a ladder given rung by rung.
//!
//! The output makes no encoder of its own. Whatever it asked for becomes a
//! `RenditionChoice` the mixer plans with every other output, so its rungs
//! share encoders with them, go to a hardware encoder when the governor
//! says so, and are counted. Each rung's keyframe interval is set to the
//! segment length, so every segment starts on a keyframe of every rung.

use super::HlsParams;
use crate::config::OutputConfig;
use anyhow::{Context, Result};
use godwinmix_protocol::rendition::{Container, LadderRef, PresetRef, RenditionChoice, RenditionRequest, VideoCodec};
use godwinmix_render::presets;

/// The provide id of the HLS output.
pub const TYPE: &str = "hls/output";

/// Whether `cfg` is an HLS output.
pub fn is_hls(cfg: &OutputConfig) -> bool {
    cfg.type_id.as_deref() == Some(TYPE)
}

/// The rendition an HLS output asks the planner for. `rendition` beside the
/// output's params, or in them (`rendition = { preset = "abr-ladder-4" }`,
/// `rendition = { ladder = [...] }`), or `ladder = "abr-ladder-4"` in them
/// for short. None serves the programme as it is.
pub fn choice(cfg: &OutputConfig) -> Result<Option<RenditionChoice>> {
    let params = cfg.effective_params();
    let asked = match &cfg.rendition {
        Some(c) => Some(c.clone()),
        None => from_params(&params)?,
    };
    let Some(asked) = asked else { return Ok(None) };
    let segment_ms = HlsParams::from_params(&params)?.segment_ms;
    let rungs = match asked {
        RenditionChoice::Preset(p) => match preset_rungs(&p.preset)? {
            Some(r) => r,
            None => return Ok(None),
        },
        RenditionChoice::Ladder(l) => l.ladder,
        RenditionChoice::Request(r) if r == RenditionRequest::default() => anyhow::bail!(
            "hls/output rendition says nothing it wants. A ladder whose rungs do not read as \
             renditions lands here too: check each rung's fields (fps is {{ num = 30, den = 1 }})."
        ),
        RenditionChoice::Request(r) => vec![r],
    };
    anyhow::ensure!(
        !rungs.is_empty(),
        "hls/output rendition.ladder is empty. Give it at least one rung, or leave it out to serve the programme as it is."
    );
    let rungs = rungs.into_iter().map(|r| rung(r, segment_ms)).collect::<Result<Vec<_>>>()?;
    Ok(Some(RenditionChoice::Ladder(LadderRef { ladder: rungs })))
}

fn from_params(params: &crate::config::Params) -> Result<Option<RenditionChoice>> {
    if let Some(r) = params.get("rendition") {
        let c: RenditionChoice = r.clone().try_into().context(
            "hls/output rendition must be { preset = \"abr-ladder-4\" }, or { ladder = [...] } with one rendition per rung",
        )?;
        return Ok(Some(c));
    }
    Ok(params
        .get("ladder")
        .and_then(|v| v.as_str())
        .map(|name| RenditionChoice::Preset(PresetRef { preset: name.to_string() })))
}

/// A preset's rungs. None for `copy`, which is the programme as it is.
fn preset_rungs(name: &str) -> Result<Option<Vec<RenditionRequest>>> {
    let found = presets::preset(name).with_context(|| {
        format!(
            "hls/output has no ladder called `{name}`. It has abr-ladder-4 (1080p, 720p, 480p, \
             360p) and abr-ladder-3 (720p, 480p, 360p), or a ladder of your own as rendition.ladder; \
             leave it out to serve the programme as it is."
        )
    })?;
    if found.id == "copy" {
        return Ok(None);
    }
    Ok(Some(found.ladder.unwrap_or_else(|| vec![found.request])))
}

/// One rung, checked, with its keyframes on every segment boundary.
fn rung(mut r: RenditionRequest, segment_ms: u32) -> Result<RenditionRequest> {
    if r.id.is_empty() {
        r.id = r.video.as_ref().and_then(|v| v.height).map_or_else(|| "programme".into(), |h| format!("{h}p"));
    }
    let slug = !r.id.is_empty() && r.id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    anyhow::ensure!(slug, "rung id `{}` must be a short slug of small letters, numbers and dashes, like 720p", r.id);
    anyhow::ensure!(r.id != "audio", "`audio` is the name of the sound every rung shares. Call the rung by its size, like 720p.");
    anyhow::ensure!(
        !r.no_video,
        "rung {} has no picture. An HLS ladder is pictures in several sizes; for sound alone use an audio output.",
        r.id
    );
    let v = r.video.get_or_insert_with(Default::default);
    if let Some(c) = v.codec.filter(|c| !matches!(c, VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Av1)) {
        anyhow::bail!("rung {} asks for {c:?}, which HLS does not carry. Ask for h264, h265 or av1.", r.id);
    }
    if let Some(h) = v.height {
        anyhow::ensure!((144..=2160).contains(&h), "rung {} is {h} lines high; a ladder takes 144 to 2160", r.id);
    }
    v.keyframe_ms = Some(segment_ms);
    r.container = Container::Hls;
    Ok(r)
}

/// The rung's name in URLs, from the planner's request id: `<output>-720p`
/// is `720p`. A single rendition is named after its height.
pub fn rung_slug(output: &str, request: &str, height: Option<u32>) -> String {
    match request.strip_prefix(output).and_then(|r| r.strip_prefix('-')) {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => height.map_or_else(|| "programme".to_string(), |h| format!("{h}p")),
    }
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
