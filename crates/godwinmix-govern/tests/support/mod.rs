//! The catalogue, turned into calibration candidates with the core's own
//! presence check and property code. This is the glue the station will
//! carry when it owns the governor; it lives here until then so the end to
//! end test measures exactly what the mixer would run.

use godwinmix_core::catalogue::apply::{self, Vars};
use godwinmix_core::catalogue::model::Role;
use godwinmix_core::catalogue::select::{GstRegistry, Registry};
use godwinmix_core::catalogue::Catalogue;
use godwinmix_govern::calibrate::{AudioCandidate, Candidate};
use godwinmix_protocol::rendition::{AudioCodec, EncoderSlot, VideoCodec};
use std::sync::Arc;

fn video_codec(name: &str) -> Option<VideoCodec> {
    Some(match name {
        "h264" => VideoCodec::H264,
        "h265" => VideoCodec::H265,
        "av1" => VideoCodec::Av1,
        "vp8" => VideoCodec::Vp8,
        "vp9" => VideoCodec::Vp9,
        _ => return None,
    })
}

fn audio_codec(name: &str) -> Option<AudioCodec> {
    Some(match name {
        "aac" => AudioCodec::Aac,
        "opus" => AudioCodec::Opus,
        "mp3" => AudioCodec::Mp3,
        _ => return None,
    })
}

fn installed(reg: &GstRegistry, names: &[String]) -> bool {
    !names.is_empty() && names.iter().all(|n| reg.has(n))
}

/// Every video encoder entry whose elements this machine has.
pub fn candidates(cat: &Catalogue) -> Vec<Candidate> {
    let reg = GstRegistry;
    let mut out = Vec::new();
    for e in cat.video.iter().filter(|e| !e.disabled) {
        let (Some(element), Some(codec)) = (e.encoder.clone(), video_codec(&e.codec)) else { continue };
        if !installed(&reg, &e.needs(Role::Encode)) {
            continue;
        }
        let hardware = e.accel != "software";
        let (props, kf) = (e.properties.clone(), e.keyframe.clone());
        out.push(Candidate {
            slot: EncoderSlot { id: e.id(), codec, hardware, device: hardware.then(|| e.accel.clone()) },
            element,
            parser: e.parser.clone().filter(|p| reg.has(p)),
            decoder: e.decoder.clone().filter(|d| reg.has(d)),
            configure: Arc::new(move |el, shape| {
                let fps = i64::from(shape.fps.num / shape.fps.den.max(1));
                let vars = Vars {
                    video_bitrate_kbps: i64::from(shape.bitrate_kbps),
                    fps,
                    keyframe_frames: fps * 2,
                    ..Vars::default()
                };
                apply::apply(el, &props, &vars);
                apply::apply_keyframe(el, kf.as_ref(), &vars);
            }),
        });
    }
    out
}

/// Every audio encoder entry whose elements this machine has, one per codec.
pub fn audio(cat: &Catalogue) -> Vec<AudioCandidate> {
    let reg = GstRegistry;
    let mut out: Vec<AudioCandidate> = Vec::new();
    for e in cat.audio.iter().filter(|e| !e.disabled) {
        let (Some(element), Some(codec)) = (e.encoder.clone(), audio_codec(&e.codec)) else { continue };
        if out.iter().any(|a| a.codec == codec) || !installed(&reg, &e.needs(Role::Encode)) {
            continue;
        }
        let props = e.properties.clone();
        out.push(AudioCandidate {
            id: e.id(),
            codec,
            element,
            configure: Arc::new(move |el| apply::apply(el, &props, &Vars::default())),
        });
    }
    out
}
