//! The audio half of resolving a request: the same rules as video, on a
//! stream that costs a hundredth as much.

use godwinmix_protocol::rendition::{AudioCodec, AudioShape, RenditionRequest, StreamInfo};

use crate::container::{audio_codecs, audio_name, audio_slug, carries_audio, container_slug};
use crate::error::PlanError;
use crate::resolve::{missing, DEFAULT_TOLERANCE};

#[derive(Debug, Clone)]
pub enum AudioDecision {
    None,
    Copy,
    Encode { target: AudioShape, why: String },
}

pub fn resolve_audio(
    req: &RenditionRequest,
    source: &str,
    info: &StreamInfo,
    available: &[AudioCodec],
) -> Result<AudioDecision, PlanError> {
    if req.no_audio {
        return Ok(AudioDecision::None);
    }
    let Some(src) = info.audio else {
        if req.audio.is_some() {
            return Err(missing(req, source, "audio"));
        }
        return Ok(AudioDecision::None);
    };
    let want = req.audio.clone().unwrap_or_default();
    let codec = audio_codec(req, info, available, want.codec)?;
    let mut target = AudioShape {
        codec,
        channels: want.channels.unwrap_or(src.channels),
        sample_rate: want.sample_rate.unwrap_or(src.sample_rate),
        bitrate_kbps: 0,
    };
    let Some(why) = mismatch(req, info, &src, &target, want.bitrate_kbps) else {
        return Ok(AudioDecision::Copy);
    };
    if want.codec.is_none() && !available.contains(&target.codec) {
        let allowed = audio_codecs(req.container);
        target.codec = allowed.iter().copied().find(|c| available.contains(c)).unwrap_or(allowed[0]);
    }
    target.bitrate_kbps = want.bitrate_kbps.unwrap_or_else(|| default_kbps(&target));
    Ok(AudioDecision::Encode { target, why })
}

fn audio_codec(
    req: &RenditionRequest,
    info: &StreamInfo,
    available: &[AudioCodec],
    asked: Option<AudioCodec>,
) -> Result<AudioCodec, PlanError> {
    let container = req.container;
    if let Some(codec) = asked {
        if !carries_audio(container, codec) {
            return Err(PlanError::ContainerCodec {
                request: req.id.clone(),
                container: container_slug(container).into(),
                codec: audio_slug(codec).into(),
                allowed: audio_codecs(container).iter().map(|c| audio_slug(*c).into()).collect(),
            });
        }
        return Ok(codec);
    }
    let own = info.audio.map(|a| a.codec).filter(|c| carries_audio(container, *c));
    if let (true, Some(codec)) = (info.encoded, own) {
        return Ok(codec);
    }
    let allowed = audio_codecs(container);
    Ok(allowed.iter().copied().find(|c| available.contains(c)).unwrap_or(allowed[0]))
}

fn mismatch(
    req: &RenditionRequest,
    info: &StreamInfo,
    src: &AudioShape,
    t: &AudioShape,
    kbps: Option<u32>,
) -> Option<String> {
    if !info.encoded {
        return Some("the source's sound is raw samples".into());
    }
    if !carries_audio(req.container, src.codec) {
        let c = container_slug(req.container);
        return Some(format!("{c} cannot carry the source's {}", audio_name(src.codec)));
    }
    if src.codec != t.codec {
        return Some(format!("the source is {} and this output wants {}", audio_name(src.codec), audio_name(t.codec)));
    }
    if (src.channels, src.sample_rate) != (t.channels, t.sample_rate) {
        return Some(format!(
            "the source is {} channels at {} Hz and this output wants {} at {} Hz",
            src.channels, src.sample_rate, t.channels, t.sample_rate
        ));
    }
    let want = kbps?;
    let off = (f64::from(src.bitrate_kbps) - f64::from(want)).abs();
    if src.bitrate_kbps > 0 && off > f64::from(want) * f64::from(DEFAULT_TOLERANCE) {
        return Some(format!("the source's sound is {} kbit/s and this output wants {want}", src.bitrate_kbps));
    }
    None
}

/// A bitrate for an output that named none, per channel and codec.
pub fn default_kbps(shape: &AudioShape) -> u32 {
    let ch = u32::from(shape.channels.max(1));
    match shape.codec {
        AudioCodec::Opus => 48 * ch,
        AudioCodec::Ac3 => 96 * ch,
        AudioCodec::Pcm => ch * shape.sample_rate * 16 / 1000,
        _ => 64 * ch,
    }
}
