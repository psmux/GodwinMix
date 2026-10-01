//! What a burst of sound is, read off its own first frame: the caps its
//! decoder takes and where the coded frame starts in each tag's body.
//!
//! The hub carries AAC as classic FLV, with a sequence header in front, and
//! AC-3, E-AC-3 and MPEG audio layer II as enhanced bodies with no header at
//! all (`crate::exaudio`); MP3 keeps classic FLV's id. A burst is therefore
//! decoded as what its frames say they are, and the AAC header is used only
//! for AAC frames: an input that changes codec between sessions keeps the
//! old header on the hub, and AAC caps on MPEG audio decode to nothing.

use gstreamer as gst;

use crate::exaudio::{self, Frame};
use crate::media_tag::MediaTag;

/// Caps for the decoder, and how many bytes of each body come before the
/// coded frame.
pub struct Coded {
    pub caps: gst::Caps,
    pub skip: usize,
}

/// Classic FLV's sound formats this reads: AAC, and MP3 (2, and 14 for MP3
/// at 8 kHz).
const FLV_AAC: u8 = 10;
const FLV_MP3: [u8; 2] = [2, 14];

/// How the burst whose first frame is `first` is decoded, or `None` for a
/// codec nothing here decodes (or AAC before its header).
pub fn coded(first: &MediaTag, aac_header: Option<&MediaTag>) -> Option<Coded> {
    let body = &first.payload;
    let format = body.first()? >> 4;
    if format == FLV_AAC {
        let header = aac_header.filter(|h| h.payload.first().is_some_and(|b| b >> 4 == FLV_AAC))?;
        return Some(Coded { caps: crate::transcode::input::caps_for(header)?, skip: 2 });
    }
    if FLV_MP3.contains(&format) {
        return mpeg(&exaudio::mpeg_audio(body.get(1..)?)?, 1);
    }
    let frame = exaudio::read(body)?;
    match &exaudio::fourcc(body)? {
        exaudio::MPEG => mpeg(&frame, 5),
        exaudio::AC3 => ac3("audio/x-ac3", &frame),
        exaudio::EAC3 => ac3("audio/x-eac3", &frame),
        _ => None,
    }
}

fn mpeg(f: &Frame, skip: usize) -> Option<Coded> {
    let layer = match f.codec {
        "mp1" => 1,
        "mp2" => 2,
        "mp3" => 3,
        _ => return None,
    };
    let caps = gst::Caps::builder("audio/mpeg")
        .field("mpegversion", 1i32)
        .field("layer", layer)
        .field("rate", f.sample_rate as i32)
        .field("channels", f.channels as i32)
        .field("parsed", true)
        .build();
    Some(Coded { caps, skip })
}

fn ac3(name: &str, f: &Frame) -> Option<Coded> {
    let caps = gst::Caps::builder(name)
        .field("rate", f.sample_rate as i32)
        .field("channels", f.channels as i32)
        .field("framed", true)
        .field("alignment", "frame")
        .build();
    Some(Coded { caps, skip: 5 })
}

/// The kind of input a chain is built for, from caps: their name, and for
/// `audio/mpeg` whether it is AAC or MPEG-1 audio, which share the name and
/// no decoder.
pub fn kind(caps: &gst::CapsRef) -> Option<String> {
    let s = caps.structure(0)?;
    let name = s.name().as_str();
    Some(match s.get::<i32>("mpegversion") {
        Ok(1) if name == "audio/mpeg" => "audio/mpeg-1".to_string(),
        _ => name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::media_tag::TagKind;

    fn tag(body: Vec<u8>, header: bool) -> MediaTag {
        MediaTag { kind: TagKind::Audio, timestamp_ms: 0, keyframe: false, sequence_header: header, payload: Arc::from(body) }
    }

    fn mp2() -> MediaTag {
        let mut body = exaudio::prefix(exaudio::MPEG).to_vec();
        body.extend_from_slice(&[0xFF, 0xFD, 0x84, 0x00, 0, 0]);
        tag(body, false)
    }

    #[test]
    fn layer_two_is_mpeg_one_audio_whatever_header_the_hub_kept() {
        gst::init().unwrap();
        let aac_header = tag(vec![0xAF, 0x00, 0x11, 0x90], true);
        let c = coded(&mp2(), Some(&aac_header)).expect("layer II is decoded");
        assert_eq!(c.skip, 5);
        let s = c.caps.structure(0).unwrap();
        assert_eq!((s.get::<i32>("mpegversion").unwrap(), s.get::<i32>("layer").unwrap()), (1, 2));
        assert_eq!((s.get::<i32>("rate").unwrap(), s.get::<i32>("channels").unwrap()), (48_000, 2));
        assert_eq!(kind(&c.caps).as_deref(), Some("audio/mpeg-1"));
    }

    #[test]
    fn aac_takes_its_header_and_waits_for_one() {
        gst::init().unwrap();
        let frame = tag(vec![0xAF, 0x01, 0x21, 0x00], false);
        assert!(coded(&frame, None).is_none(), "no header yet");
        let c = coded(&frame, Some(&tag(vec![0xAF, 0x00, 0x11, 0x90], true))).unwrap();
        assert_eq!((c.skip, kind(&c.caps).as_deref()), (2, Some("audio/mpeg")));
    }

    #[test]
    fn classic_mp3_and_ac3_are_read_from_their_frames() {
        gst::init().unwrap();
        let mp3 = coded(&tag(vec![0x2F, 0xFF, 0xFB, 0x90, 0xC0], false), None).unwrap();
        assert_eq!((mp3.skip, mp3.caps.structure(0).unwrap().get::<i32>("layer").unwrap()), (1, 3));
        let mut body = exaudio::prefix(exaudio::AC3).to_vec();
        body.extend_from_slice(&[0x0B, 0x77, 0, 0, 0x00, 0x40, 0x40, 0x00]);
        let ac3 = coded(&tag(body, false), None).unwrap();
        assert_eq!((ac3.skip, kind(&ac3.caps).as_deref()), (5, Some("audio/x-ac3")));
    }
}
