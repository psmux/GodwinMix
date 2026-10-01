//! The sound a tag carries, as MPEG-TS wants it: which stream type and PES
//! stream id, and the bytes of one access unit.
//!
//! AAC arrives raw with its AudioSpecificConfig in the sequence header and
//! goes out with an ADTS header on every frame. AC-3, E-AC-3 and MPEG audio
//! (layers II and III) arrive as enhanced RTMP v2 bodies (`crate::exaudio`),
//! or MP3 as classic FLV, each frame already carrying its own sync header, so
//! they go out as they came. AC-3 and E-AC-3 use the ATSC stream types and
//! private stream 1, which ffmpeg, VLC and GStreamer all read.

use super::es::AudioConfig;
use crate::exaudio;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Aac(AudioConfig),
    Ac3,
    Eac3,
    /// MPEG audio, with its stream type: 0x03 for MPEG-1, 0x04 for MPEG-2.
    Mpeg(u8),
}

impl Sound {
    pub fn stream_type(&self) -> u8 {
        match self {
            Sound::Aac(_) => 0x0f,
            Sound::Ac3 => 0x81,
            Sound::Eac3 => 0x87,
            Sound::Mpeg(t) => *t,
        }
    }

    pub fn stream_id(&self) -> u8 {
        match self {
            Sound::Ac3 | Sound::Eac3 => 0xbd,
            _ => 0xc0,
        }
    }

    /// The sound of a frame that needs no sequence header, and where its
    /// frame starts in the body. `None` for AAC and anything not carried.
    pub fn of_frame(body: &[u8]) -> Option<(Sound, usize)> {
        let first = *body.first()?;
        if first >> 4 == 2 {
            // Classic FLV MP3: one byte of flags, then the frame.
            return Some((Sound::Mpeg(mpeg_type(body.get(1..)?)), 1));
        }
        match &exaudio::fourcc(body)? {
            exaudio::AC3 => Some((Sound::Ac3, 5)),
            exaudio::EAC3 => Some((Sound::Eac3, 5)),
            exaudio::MPEG => Some((Sound::Mpeg(mpeg_type(body.get(5..)?)), 5)),
            _ => None,
        }
    }

    /// One access unit's bytes, the frame starting at `at` in `body`.
    pub fn unit(&self, body: &[u8], at: usize, out: &mut Vec<u8>) {
        let Some(frame) = body.get(at..) else { return };
        if let Sound::Aac(config) = self {
            out.extend_from_slice(&config.adts(frame.len()));
        }
        out.extend_from_slice(frame);
    }
}

/// MPEG-1 audio is stream type 3; MPEG-2 and 2.5, at half and quarter rate, 4.
fn mpeg_type(frame: &[u8]) -> u8 {
    match frame.get(1).map(|b| (b >> 3) & 3) {
        Some(3) | None => 0x03,
        _ => 0x04,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_sound_is_told_by_its_body_and_gets_its_stream_type() {
        let ac3 = [&exaudio::prefix(exaudio::AC3)[..], &[0x0B, 0x77, 0, 0]].concat();
        assert_eq!(Sound::of_frame(&ac3), Some((Sound::Ac3, 5)));
        assert_eq!((Sound::Ac3.stream_type(), Sound::Ac3.stream_id()), (0x81, 0xbd));
        let mp2 = [&exaudio::prefix(exaudio::MPEG)[..], &[0xFF, 0xFD, 0x84, 0x00]].concat();
        assert_eq!(Sound::of_frame(&mp2), Some((Sound::Mpeg(0x03), 5)));
        assert_eq!(Sound::of_frame(&[0x2F, 0xFF, 0xF3, 0x90]), Some((Sound::Mpeg(0x04), 1)), "classic FLV MP3, MPEG-2");
        assert_eq!(Sound::of_frame(&[0xAF, 1, 0x21]), None, "AAC needs its header");
        let mut out = Vec::new();
        Sound::Ac3.unit(&ac3, 5, &mut out);
        assert_eq!(out, [0x0B, 0x77, 0, 0], "self synced frames go out as they came");
    }
}
