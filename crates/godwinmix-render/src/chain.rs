//! One request's chain, from its source to its Mux, built on shared nodes.

use godwinmix_protocol::rendition::{AudioShape, Container, VideoShape};

use crate::audio::AudioDecision;
use crate::build::Builder;
use crate::container::{audio_codecs, audio_name};
use crate::error::PlanError;
use crate::graph::{Node, NodeKind, Reason, ReasonCode, Track};
use crate::ids::{aconvert_id, aencode_id, track_source};
use crate::model::{AudioWork, CPU};
use crate::plan::Resolved;
use crate::resolve::VideoDecision;

/// Builds the whole chain for one request, ending in its Mux node.
pub fn build(b: &mut Builder, r: &Resolved, keyframe_ms: u32) -> Result<(), PlanError> {
    let mut inputs = Vec::new();
    let mut egress = 0;
    if let Some((id, kbps)) = video(b, r, keyframe_ms)? {
        inputs.push(id);
        egress += kbps;
    }
    if let Some(src) = r.info.audio {
        if let Some((id, kbps)) = audio(b, r, 0, src, &r.audio)? {
            inputs.push(id);
            egress += kbps;
        }
    }
    for (index, src, decision) in &r.tracks {
        if let Some((id, kbps)) = audio(b, r, *index, *src, decision)? {
            inputs.push(id);
            egress += kbps;
        }
    }
    let request = r.id().to_string();
    let container = r.request.container;
    let cost = b.model.mux_cost(container, egress);
    let kind = NodeKind::Mux {
        request: request.clone(),
        container,
    };
    let id = format!("mux:{request}");
    let reason = Some(reason(r));
    b.push(Node {
        id,
        kind,
        inputs,
        serves: vec![request],
        device: CPU.into(),
        cost,
        reason,
    });
    Ok(())
}

fn video(
    b: &mut Builder,
    r: &Resolved,
    keyframe_ms: u32,
) -> Result<Option<(String, u32)>, PlanError> {
    let Some(src) = r.info.video else {
        return Ok(None);
    };
    match &r.video {
        VideoDecision::None => Ok(None),
        VideoDecision::Copy => Ok(Some((b.copied(r, Track::Video), src.bitrate_kbps))),
        VideoDecision::Encode { target, .. } => {
            let target = VideoShape {
                keyframe_ms,
                ..*target
            };
            Ok(Some((b.encoded(r, &target, &src)?, target.bitrate_kbps)))
        }
    }
}

/// One sound track's chain: track 0 is the source's first sound, as it
/// always was; 1, 2, 3 are the further tracks of a request for all.
fn audio(
    b: &mut Builder,
    r: &Resolved,
    index: u8,
    src: AudioShape,
    decision: &AudioDecision,
) -> Result<Option<(String, u32)>, PlanError> {
    let target = match decision {
        AudioDecision::None => return Ok(None),
        AudioDecision::Copy => {
            let id = b.copied_track(r, Track::Audio, index);
            return Ok(Some((id, src.bitrate_kbps)));
        }
        AudioDecision::Encode { target, .. } => *target,
    };
    let model = b.model;
    let Some(cost) = model.audio_cost(&target, AudioWork::Encode) else {
        return Err(no_audio_encoder(b, r, &target));
    };
    let mut upstream = b.decoded_track(r, index, &src);
    let source = r.source.to_string();
    let named = track_source(&source, index);
    let (channels, sample_rate) = (target.channels, target.sample_rate);
    if (channels, sample_rate) != (src.channels, src.sample_rate) {
        let id = aconvert_id(&named, &target);
        let s = source.clone();
        upstream = b.cpu(id, r.id(), vec![upstream], || {
            let cost = model
                .audio_cost(&target, AudioWork::Convert)
                .unwrap_or_default();
            (
                NodeKind::AudioConvert {
                    source: s,
                    channels,
                    sample_rate,
                    index,
                },
                cost,
            )
        });
    }
    let id = aencode_id(&named, &target);
    let id = b.cpu(id, r.id(), vec![upstream], || {
        (
            NodeKind::AudioEncode {
                source,
                shape: target,
                index,
            },
            cost,
        )
    });
    Ok(Some((id, target.bitrate_kbps)))
}

fn no_audio_encoder(b: &Builder, r: &Resolved, target: &AudioShape) -> PlanError {
    let container: Container = r.request.container;
    let nearest = audio_codecs(container)
        .iter()
        .find(|c| {
            let alt = AudioShape {
                codec: **c,
                ..*target
            };
            b.model.audio_cost(&alt, AudioWork::Encode).is_some()
        })
        .map(|c| audio_name(*c).to_string());
    PlanError::NoAudioEncoder {
        request: r.id().into(),
        codec: audio_name(target.codec).into(),
        nearest,
    }
}

/// The sentence on a Mux node: copied, or what made it an encode.
fn reason(r: &Resolved) -> Reason {
    let (copied, why) = match (&r.video, &r.audio) {
        (VideoDecision::Encode { why, .. }, _) => (false, why.as_str()),
        (VideoDecision::Copy, _) => (true, "the source's video goes out as it is"),
        (VideoDecision::None, AudioDecision::Encode { why, .. }) => (false, why.as_str()),
        (VideoDecision::None, _) => (true, "the source's sound goes out as it is"),
    };
    if copied {
        return Reason {
            code: ReasonCode::Copied,
            text: format!("copied: {why}"),
        };
    }
    Reason {
        code: ReasonCode::Transcoded,
        text: format!("encoded because {why}"),
    }
}
