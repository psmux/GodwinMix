//! A refusal that helps: what the work needs, what is free, and what would
//! fit instead, as a sentence for a person and a list for a program.

use crate::calibration::mpix;
use crate::headroom::short;
use crate::profile::Profile;
use godwinmix_protocol::rendition::{Cost, EncoderSlot, Fps, VideoCodec, VideoShape};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Advice {
    /// One or two sentences a person reads.
    pub text: String,
    /// What is short: `cpu`, `device`, `sessions`, `memory`, `uplink`.
    pub short: Vec<&'static str>,
    /// Renditions that would fit now, the largest first.
    pub fits: Vec<Fit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fit {
    /// `720p30 H.264 on h264-software-x264`, `1080p30 H.264 on the GPU
    /// encoder h264-videotoolbox`.
    pub label: String,
    pub slot: EncoderSlot,
    pub width: u32,
    pub height: u32,
    pub fps: Fps,
    pub cost: Cost,
}

/// The shapes offered as alternatives, largest first.
const SHAPES: &[(u32, u32, u32)] = &[(1920, 1080, 60), (1920, 1080, 30), (1280, 720, 60), (1280, 720, 30), (854, 480, 30)];

/// Advice for a refusal. `have_on(device)` is what is free with that device
/// as the one the work would run on.
pub fn advise(what: &str, need: &Cost, have: &Cost, profile: &Profile, have_on: impl Fn(Option<&str>) -> Cost) -> Advice {
    let short = short(need, have);
    let fits = fits(profile, &have_on);
    let mut text = format!("{what} needs {} and {} is free.", amount(need, &short), amount(have, &short));
    let soft = fits.iter().find(|f| !f.slot.hardware);
    let hard = fits.iter().find(|f| f.slot.hardware);
    match (soft, hard) {
        (Some(s), Some(h)) => text.push_str(&format!(" {} fits, or {}.", s.label, h.label)),
        (Some(f), None) | (None, Some(f)) => text.push_str(&format!(" {} fits.", f.label)),
        (None, None) => text.push_str(" Nothing more fits now: stop a preview or an output, or make one that is running smaller."),
    }
    if !profile.is_calibrated() {
        text.push_str(" This machine has not been measured yet, so these figures are cautious guesses.");
    }
    Advice { text, short, fits }
}

fn fits(profile: &Profile, have_on: &impl Fn(Option<&str>) -> Cost) -> Vec<Fit> {
    let mut out = Vec::new();
    for slot in all_slots(profile) {
        let have = have_on(slot.device.as_deref());
        let best = SHAPES.iter().find_map(|&(w, h, f)| {
            let shape = VideoShape { codec: slot.codec, width: w, height: h, fps: Fps::whole(f), bitrate_kbps: 0, keyframe_ms: 0 };
            let cost = profile.encode_cost(&slot, &shape);
            short(&cost, &have).is_empty().then_some((shape, cost))
        });
        if let Some((s, cost)) = best {
            let on = if slot.hardware { "the GPU encoder " } else { "" };
            let label = format!("{}p{} {} on {on}{}", s.height, s.fps.num, codec_name(slot.codec), slot.id);
            out.push(Fit { label, slot, width: s.width, height: s.height, fps: s.fps, cost });
        }
    }
    // Largest picture first; H.264 ahead of the rest at the same size, since
    // every platform takes it.
    out.sort_by(|a, b| {
        let (pa, pb) = (mpix(a.width, a.height, a.fps), mpix(b.width, b.height, b.fps));
        pb.total_cmp(&pa).then((a.slot.codec != VideoCodec::H264).cmp(&(b.slot.codec != VideoCodec::H264)))
    });
    out
}

fn all_slots(profile: &Profile) -> Vec<EncoderSlot> {
    [VideoCodec::H264, VideoCodec::H265, VideoCodec::Av1, VideoCodec::Vp9, VideoCodec::Vp8]
        .into_iter()
        .flat_map(|c| profile.encoders(c))
        .collect()
}

/// The parts of a cost that matter to this refusal, in words.
fn amount(c: &Cost, short: &[&str]) -> String {
    let mut parts = Vec::new();
    for s in short {
        parts.push(match *s {
            "cpu" => format!("{:.1} cores", f64::from(c.cpu_millicores) / 1000.0),
            "device" => format!("{}% of the GPU encoder", c.device_millis / 10),
            "sessions" if c.device_sessions == crate::headroom::UNLIMITED => "any number of encoder sessions".into(),
            "sessions" => format!("{} encoder sessions", c.device_sessions),
            "memory" => format!("{} MiB of memory", c.memory_mib),
            _ => format!("{} kbit/s of uplink", c.egress_kbps),
        });
    }
    if parts.is_empty() {
        parts.push(format!("{:.1} cores", f64::from(c.cpu_millicores) / 1000.0));
    }
    parts.join(" and ")
}

pub fn codec_name(c: VideoCodec) -> &'static str {
    match c {
        VideoCodec::H264 => "H.264",
        VideoCodec::H265 => "HEVC",
        VideoCodec::Av1 => "AV1",
        VideoCodec::Vp8 => "VP8",
        VideoCodec::Vp9 => "VP9",
        VideoCodec::Mpeg2 => "MPEG-2",
        VideoCodec::Prores => "ProRes",
        VideoCodec::Other => "video",
    }
}
