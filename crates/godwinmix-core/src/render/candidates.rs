//! The codec catalogue as the governor's calibration candidates and as the
//! planner's encoder list: the catalogue's own presence check and property
//! code, so there is one list of encoders and one way to configure them.
//!
//! `[hardware] encode = "software"` (or any other pin) narrows the list the
//! same way it narrows the programme encoder, which is how a machine with a
//! GPU is made to behave like one without.

use crate::catalogue::apply::{self, Vars};
use crate::catalogue::model::{AudioEntry, Role, VideoEntry};
use crate::catalogue::select::{GstRegistry, Registry};
use crate::catalogue::Catalogue;
use crate::config::Accel;
use godwinmix_govern::calibrate::{AudioCandidate, Candidate};
use godwinmix_protocol::rendition::{AudioCodec, EncoderSlot, VideoCodec, VideoShape};
use std::sync::Arc;

pub fn video_codec(name: &str) -> Option<VideoCodec> {
    Some(match name {
        "h264" => VideoCodec::H264,
        "h265" => VideoCodec::H265,
        "av1" => VideoCodec::Av1,
        "vp8" => VideoCodec::Vp8,
        "vp9" => VideoCodec::Vp9,
        _ => return None,
    })
}

pub fn audio_codec(name: &str) -> Option<AudioCodec> {
    Some(match name {
        "aac" => AudioCodec::Aac,
        "opus" => AudioCodec::Opus,
        "mp3" => AudioCodec::Mp3,
        _ => return None,
    })
}

fn installed(reg: &dyn Registry, names: &[String]) -> bool {
    !names.is_empty() && names.iter().all(|n| reg.has(n))
}

/// The catalogue's variables for one encode of `shape`, with the keyframe
/// interval the ladder settled on.
pub fn vars_for(shape: &VideoShape, audio_kbps: u32) -> Vars {
    let fps = i64::from(shape.fps.num / shape.fps.den.max(1)).max(1);
    let secs = i64::from(shape.keyframe_ms.max(1000) / 1000);
    Vars {
        video_bitrate_kbps: i64::from(shape.bitrate_kbps),
        audio_bitrate_kbps: i64::from(audio_kbps),
        keyframe_frames: fps * i64::from(shape.keyframe_ms.max(1)) / 1000,
        keyframe_secs: secs,
        fps,
        ..Vars::default()
    }
}

/// The slot the planner and the governor know this entry by.
pub fn slot_of(e: &VideoEntry) -> Option<EncoderSlot> {
    let codec = video_codec(&e.codec)?;
    e.encoder.as_ref()?;
    let hardware = e.accel != "software";
    Some(EncoderSlot {
        id: e.id(),
        codec,
        hardware,
        device: hardware.then(|| e.accel.clone()),
    })
}

/// The video encoder entries this machine has, in catalogue rank order,
/// within the `[hardware] encode` pin.
pub fn video_entries<'a>(cat: &'a Catalogue, pin: Accel, reg: &dyn Registry) -> Vec<&'a VideoEntry> {
    let mut v: Vec<&VideoEntry> = cat
        .video
        .iter()
        .filter(|e| !e.disabled && e.encoder.is_some())
        .filter(|e| pin.name().is_none_or(|p| p == e.accel))
        .filter(|e| installed(reg, &e.needs(Role::Encode)))
        .collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.rank));
    v
}

/// Every video encoder entry whose elements this machine has.
pub fn candidates(cat: &Catalogue, pin: Accel) -> Vec<Candidate> {
    let reg = GstRegistry;
    let mut out = Vec::new();
    for e in video_entries(cat, pin, &reg) {
        let (Some(slot), Some(element)) = (slot_of(e), e.encoder.clone()) else { continue };
        let (props, kf) = (e.properties.clone(), e.keyframe.clone());
        out.push(Candidate {
            slot,
            element,
            parser: e.parser.clone().filter(|p| reg.has(p)),
            decoder: e.decoder.clone().filter(|d| reg.has(d)),
            configure: Arc::new(move |el, shape| {
                let shape = VideoShape { keyframe_ms: 2000, ..*shape };
                let vars = vars_for(&shape, 128);
                apply::apply(el, &props, &vars);
                apply::apply_keyframe(el, kf.as_ref(), &vars);
            }),
        });
    }
    out
}

/// The audio encoder entries this machine has, best first.
pub fn audio_entries<'a>(cat: &'a Catalogue, reg: &dyn Registry) -> Vec<&'a AudioEntry> {
    let mut v: Vec<&AudioEntry> = cat
        .audio
        .iter()
        .filter(|e| !e.disabled && e.encoder.is_some())
        .filter(|e| installed(reg, &e.needs(Role::Encode)))
        .collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.rank));
    v
}

/// Every audio encoder entry whose elements this machine has, one per codec.
pub fn audio(cat: &Catalogue) -> Vec<AudioCandidate> {
    let mut out: Vec<AudioCandidate> = Vec::new();
    for e in audio_entries(cat, &GstRegistry) {
        let (Some(element), Some(codec)) = (e.encoder.clone(), audio_codec(&e.codec)) else { continue };
        if out.iter().any(|a| a.codec == codec) {
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
