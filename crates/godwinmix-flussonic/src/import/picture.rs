//! One video track of a `transcoder` line as a picture's want.

use super::codecs::{fps_of, video_codec};
use super::want::{kbps, NVENC_PRESET};
use super::Note;
use crate::model::{Hw, SizeMode, Transcoder, VideoTrack};
use serde_json::{json, Map, Value};

/// What one video track of the line says: its codec, bitrate, size and
/// fit, rate, preset and B frames. None for `vb=copy`.
pub(super) fn picture(track: &VideoTrack, t: &Transcoder, line: usize, notes: &mut Vec<Note>) -> Option<Map<String, Value>> {
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
    for (what, set) in [("vprofile", track.profile.is_some()), ("level", track.level.is_some())] {
        if set {
            notes.push(Note::not_carried(line, format!("transcoder option {what}: the encoder's own is used")));
        }
    }
    Some(v)
}
