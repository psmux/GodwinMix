//! A `transcoder` line as an HLS output's rendition request. The one place
//! the import names rendition fields, so a rename on the media side is a
//! change here and nowhere else.
//!
//! Every field written here is `VideoWant`'s or `AudioWant`'s and acted on.
//! `hw=nvenc` has no field: the planner already takes a hardware encoder
//! first, in catalogue rank order, and a station that must never encode in
//! software pins it with `[hardware]`. `deviceid=N` is `video.gpu`: the card
//! the decode, the scale and the encode run on, either N as written
//! ([`DeviceId::File`]) or `auto`, spread by load ([`DeviceId::Spread`]).
//! `deinterlace=false`, Flussonic's default, is `auto` unless the station
//! reads it as written ([`Deinterlacing`]). Several video tracks on one
//! line (`vb=4500k size=1920x1080 vb=1500k size=1280x720`) are a ladder,
//! each a variant of the HLS output's master. What GodwinMix does
//! differently is a note: a pad colour other than black, and one picture
//! per field.

use super::codecs::audio_codec;
use super::picture::picture;
use super::{Deinterlacing, DeviceId, Note, Options};
use crate::model::{Bitrate, Deinterlace, DeinterlaceRate, Hw, Transcoder};
use serde_json::{json, Map, Value};

/// The NVENC preset for a transcoder line that names none. The catalogue's
/// own default (p4) is tuned for one encode; a headend runs dozens on one
/// card, and measured on an RTX 3060 class card p4 filled the encoder before
/// 46 channels of 720p while p1 ran them with room. A line's own `preset=`
/// is passed through as written.
pub const NVENC_PRESET: &str = "p1";

pub(super) fn kbps(b: Bitrate) -> Option<u64> {
    match b {
        Bitrate::Copy => None,
        Bitrate::Bps(bps) => Some(bps.div_ceil(1000)),
    }
}

/// The line's pictures, first first: each video track one, every one with
/// what the line says for all of them (deinterlace, gop, the card). More
/// than one is a ladder, each a variant of the HLS output's master.
fn video(t: &Transcoder, line: usize, notes: &mut Vec<Note>, opts: &Options) -> Vec<Value> {
    let mut all = Map::new();
    let deinterlace = match (t.deinterlace, opts.deinterlace) {
        (Deinterlace::Off, Deinterlacing::File) => "off",
        (Deinterlace::Off | Deinterlace::Auto, _) => "auto",
        (Deinterlace::On, _) => "on",
    };
    all.insert("deinterlace".into(), json!(deinterlace));
    if t.deinterlace_rate == DeinterlaceRate::Field && deinterlace != "off" {
        notes.push(Note::not_carried(line, "deinterlace_rate=field (one picture per field): pictures are kept at the source's frame rate".into()));
    }
    if let Some(gop) = t.gop {
        all.insert("keyframe_frames".into(), json!(gop));
    }
    match opts.deviceid {
        DeviceId::Spread => all.insert("gpu".into(), json!("auto")),
        DeviceId::File => all.insert("gpu".into(), json!(t.device_id)),
    };
    if matches!(t.hw, Hw::Qsv | Hw::Other) && t.video.iter().any(|v| kbps(v.bitrate).is_some()) {
        notes.push(Note::not_carried(line, "hw other than nvenc or cpu: the planner picks the encoder".into()));
    }
    let mut out = Vec::new();
    for track in &t.video {
        let Some(own) = picture(track, t, line, notes) else { continue };
        let mut v = out.first().and_then(Value::as_object).cloned().unwrap_or_else(|| all.clone());
        v.extend(own);
        out.push(Value::Object(v));
    }
    out
}

fn audio(t: &Transcoder, line: usize, notes: &mut Vec<Note>) -> Option<Value> {
    let track = t.audio.first()?;
    if t.audio.len() > 1 {
        notes.push(Note::not_carried(line, format!("{} audio encodes: the first is used for every sound track", t.audio.len())));
    }
    let kbps = kbps(track.bitrate)?;
    let mut a = Map::new();
    match audio_codec(&track.codec) {
        Some(c) => { a.insert("codec".into(), json!(c)); },
        None => notes.push(Note::not_carried(line, format!("acodec={}: AAC is made", track.codec))),
    }
    a.insert("bitrate_kbps".into(), json!(kbps));
    if let Some(ch) = track.channels {
        a.insert("channels".into(), json!(ch));
    }
    if let Some(rate) = track.sample_rate {
        a.insert("sample_rate".into(), json!(rate));
    }
    // Flussonic encodes every sound track the source has.
    a.insert("tracks".into(), json!("all"));
    Some(Value::Object(a))
}

/// The rendition for one transcoder line. None means a copy, which is
/// what a line with only `copy` bitrates asks for.
pub fn rendition(t: &Transcoder, line: usize, notes: &mut Vec<Note>, opts: &Options) -> Option<Value> {
    for opt in &t.unused {
        notes.push(Note::ignored(line, format!("transcoder option {opt}")));
    }
    let (pictures, a) = (video(t, line, notes, opts), audio(t, line, notes));
    if pictures.is_empty() && a.is_none() {
        return None;
    }
    let one = |v: Option<&Value>| {
        let mut r = Map::new();
        if let Some(v) = v {
            r.insert("video".into(), v.clone());
        }
        if let Some(a) = &a {
            r.insert("audio".into(), a.clone());
        }
        Value::Object(r)
    };
    if pictures.len() > 1 {
        return Some(json!({"ladder": pictures.iter().map(|v| one(Some(v))).collect::<Vec<_>>()}));
    }
    Some(one(pictures.first()))
}
