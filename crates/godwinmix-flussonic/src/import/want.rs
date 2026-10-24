//! A `transcoder` line as an HLS output's rendition request. The one place
//! the import names rendition fields, so a rename on the media side is a
//! change here and nowhere else.
//!
//! Every field written here is `VideoWant`'s or `AudioWant`'s and acted on.
//! `hw=nvenc` and `deviceid` have no field: the planner already takes a
//! hardware encoder first, in catalogue rank order, and a station that must
//! never encode in software pins it with `[hardware]`. What GodwinMix does
//! differently is a note: a pad colour other than black, one picture per
//! field, and deinterlacing, which runs on the CPU.

use super::codecs::{audio_codec, fps_of, video_codec};
use super::Note;
use crate::model::{Bitrate, Deinterlace, DeinterlaceRate, Hw, SizeMode, Transcoder};
use serde_json::{json, Map, Value};

/// The NVENC preset for a transcoder line that names none. The catalogue's
/// own default (p4) is tuned for one encode; a headend runs dozens on one
/// card, and measured on an RTX 3060 class card p4 filled the encoder before
/// 46 channels of 720p while p1 ran them with room. A line's own `preset=`
/// is passed through as written.
pub const NVENC_PRESET: &str = "p1";

fn kbps(b: Bitrate) -> Option<u64> {
    match b {
        Bitrate::Copy => None,
        Bitrate::Bps(bps) => Some(bps.div_ceil(1000)),
    }
}

fn video(t: &Transcoder, line: usize, notes: &mut Vec<Note>) -> Option<Value> {
    let track = t.video.first()?;
    if t.video.len() > 1 {
        notes.push(Note::not_carried(line, format!("{} video tracks (a ladder): an HLS output of a direct show carries one, the first", t.video.len())));
    }
    let kbps = kbps(track.bitrate)?;
    let mut v = Map::new();
    match video_codec(&track.codec) {
        Some(c) => { v.insert("codec".into(), json!(c)); },
        None => notes.push(Note::not_carried(line, format!("vcodec={}: the source's codec family is kept", track.codec))),
    }
    v.insert("bitrate_kbps".into(), json!(kbps));
    if let Some(size) = track.size {
        if size.width > 0 {
            v.insert("width".into(), json!(size.width));
        }
        if size.height > 0 {
            v.insert("height".into(), json!(size.height));
        }
        let fit = match size.mode {
            SizeMode::Fit => "pad",
            SizeMode::Crop => "crop",
            SizeMode::Scale => "stretch",
        };
        v.insert("fit".into(), json!(fit));
        if let Some(rgb) = size.pad.filter(|_| size.mode == SizeMode::Fit) {
            v.insert("pad_color".into(), json!(format!("#{rgb:06x}")));
            if rgb != 0 {
                notes.push(Note::not_carried(line, format!("pad colour #{rgb:06x}: the bars are drawn black")));
            }
        }
    }
    if let Some(fps) = track.fps {
        v.insert("fps".into(), fps_of(fps));
    }
    let deinterlace = match t.deinterlace {
        Deinterlace::Off => "off",
        Deinterlace::On => "on",
        Deinterlace::Auto => "auto",
    };
    v.insert("deinterlace".into(), json!(deinterlace));
    if t.deinterlace != Deinterlace::Off {
        notes.push(Note::not_carried(line, format!("deinterlace={deinterlace}: the deinterlacing runs on the CPU, between the GPU decode and the encode")));
    }
    if t.deinterlace_rate == DeinterlaceRate::Field && t.deinterlace != Deinterlace::Off {
        notes.push(Note::not_carried(line, "deinterlace_rate=field (one picture per field): pictures are kept at the source's frame rate".into()));
    }
    if let Some(gop) = t.gop {
        v.insert("keyframe_frames".into(), json!(gop));
    }
    match (&track.preset, t.hw) {
        (Some(p), _) => {
            v.insert("preset".into(), json!(p));
        }
        (None, Hw::Nvenc) => {
            v.insert("preset".into(), json!(NVENC_PRESET));
        }
        (None, _) => {}
    }
    if let Some(b) = track.bframes {
        v.insert("bframes".into(), json!(b));
    }
    if matches!(t.hw, Hw::Qsv | Hw::Other) {
        notes.push(Note::not_carried(line, "hw other than nvenc or cpu: the planner picks the encoder".into()));
    }
    for (what, set) in [("vprofile", track.profile.is_some()), ("level", track.level.is_some())] {
        if set {
            notes.push(Note::not_carried(line, format!("transcoder option {what}: the encoder's own is used")));
        }
    }
    Some(Value::Object(v))
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
pub fn rendition(t: &Transcoder, line: usize, notes: &mut Vec<Note>) -> Option<Value> {
    for opt in &t.unused {
        notes.push(Note::ignored(line, format!("transcoder option {opt}")));
    }
    let (v, a) = (video(t, line, notes), audio(t, line, notes));
    if v.is_none() && a.is_none() {
        return None;
    }
    let mut r = Map::new();
    if let Some(v) = v {
        r.insert("video".into(), v);
    }
    if let Some(a) = a {
        r.insert("audio".into(), a);
    }
    Some(Value::Object(r))
}
