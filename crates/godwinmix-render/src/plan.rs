//! `plan`: resolve every request, settle one keyframe interval per source,
//! then build each request's chain on shared nodes.

use std::collections::{BTreeMap, HashSet};

use godwinmix_protocol::rendition::{AudioCodec, AudioShape, RenditionRequest, StreamInfo, VideoCodec};

use crate::audio::{resolve_audio, AudioDecision};
use crate::build::Builder;
use crate::chain;
use crate::error::PlanError;
use crate::graph::Plan;
use crate::model::{AudioWork, CostModel};
use crate::resolve::{resolve_video, VideoDecision};

/// A source's slug, `cam-wide`.
pub type SourceId = String;

/// The keyframe interval a ladder gets when nothing asks for one.
pub const DEFAULT_KEYFRAME_MS: u32 = 2000;

/// One request with its source found and its copy or encode decided.
pub struct Resolved<'a> {
    pub request: &'a RenditionRequest,
    pub source: &'a str,
    pub info: &'a StreamInfo,
    pub video: VideoDecision,
    pub audio: AudioDecision,
}

impl Resolved<'_> {
    pub fn id(&self) -> &str {
        &self.request.id
    }
}

/// Plans every request against the sources they read. See the crate docs.
pub fn plan(
    sources: &[(SourceId, StreamInfo)],
    requests: &[(SourceId, RenditionRequest)],
    model: &dyn CostModel,
) -> Result<Plan, PlanError> {
    let encoders = model.encoders();
    let mut video_codecs: Vec<VideoCodec> = encoders.iter().map(|e| e.codec).collect();
    video_codecs.dedup();
    let audio_codecs = audio_available(model);
    let mut seen = HashSet::new();
    let mut resolved = Vec::with_capacity(requests.len());
    for (source, request) in requests {
        if !seen.insert(request.id.as_str()) {
            return Err(PlanError::DuplicateRequest { request: request.id.clone() });
        }
        resolved.push(resolve(sources, source, request, &video_codecs, &audio_codecs)?);
    }
    let ladder = ladder(&resolved);
    let mut b = Builder::new(model, encoders);
    for r in &resolved {
        let keyframe_ms = ladder.get(r.source).copied().unwrap_or(DEFAULT_KEYFRAME_MS);
        chain::build(&mut b, r, keyframe_ms)?;
    }
    Ok(finish(b, ladder))
}

fn resolve<'a>(
    sources: &'a [(SourceId, StreamInfo)],
    source: &'a str,
    request: &'a RenditionRequest,
    video_codecs: &[VideoCodec],
    audio_codecs: &[AudioCodec],
) -> Result<Resolved<'a>, PlanError> {
    let Some((_, info)) = sources.iter().find(|(id, _)| id == source) else {
        let known = sources.iter().map(|(id, _)| id.clone()).collect();
        return Err(PlanError::UnknownSource { request: request.id.clone(), source: source.into(), known });
    };
    let video = resolve_video(request, source, info, video_codecs)?;
    let audio = resolve_audio(request, source, info, audio_codecs)?;
    if matches!((&video, &audio), (VideoDecision::None, AudioDecision::None)) {
        return Err(PlanError::NothingAsked { request: request.id.clone() });
    }
    Ok(Resolved { request, source, info, video, audio })
}

/// The audio codecs this machine can encode, asked once per plan.
fn audio_available(model: &dyn CostModel) -> Vec<AudioCodec> {
    let all = [AudioCodec::Aac, AudioCodec::Opus, AudioCodec::Mp3, AudioCodec::Ac3, AudioCodec::Pcm];
    all.into_iter()
        .filter(|codec| {
            let shape = AudioShape { codec: *codec, channels: 2, sample_rate: 48_000, bitrate_kbps: 0 };
            model.audio_cost(&shape, AudioWork::Encode).is_some()
        })
        .collect()
}

/// One keyframe interval per source that has any encode: the shortest any
/// encoded rung asked for (2 seconds when none asked), or a copied rung's
/// own interval when that is shorter, so every rung meets its platform's
/// maximum and the encodes line up with the copy.
pub fn ladder(resolved: &[Resolved]) -> BTreeMap<String, u32> {
    let mut asked: BTreeMap<&str, u32> = BTreeMap::new();
    let mut copied: BTreeMap<&str, u32> = BTreeMap::new();
    for r in resolved {
        let (map, ms) = match &r.video {
            VideoDecision::Encode { target, .. } => (&mut asked, target.keyframe_ms),
            VideoDecision::Copy => (&mut copied, r.info.video.map_or(0, |v| v.keyframe_ms)),
            VideoDecision::None => continue,
        };
        let entry = map.entry(r.source).or_insert(0);
        *entry = shortest(*entry, ms);
    }
    let pick = |s: &str, ms: u32| {
        let ms = if ms == 0 { DEFAULT_KEYFRAME_MS } else { ms };
        shortest(ms, copied.get(s).copied().unwrap_or(0))
    };
    asked.into_iter().map(|(s, ms)| (s.to_string(), pick(s, ms))).collect()
}

/// The smaller of two intervals, where 0 means "no view".
fn shortest(a: u32, b: u32) -> u32 {
    match (a, b) {
        (0, x) | (x, 0) => x,
        (a, b) => a.min(b),
    }
}

fn finish(b: Builder, keyframe_ms: BTreeMap<String, u32>) -> Plan {
    let mut plan = Plan { nodes: b.nodes, keyframe_ms, ..Plan::default() };
    for node in &plan.nodes {
        let entry = plan.cost.entry(node.device.clone()).or_default();
        *entry = entry.plus(node.cost);
        plan.total = plan.total.plus(node.cost);
    }
    plan
}
