//! What a stream carries, read from the tags themselves.
//!
//! The first byte of an FLV video or audio body names the codec, and the
//! sequence header that follows it holds the rest: the H.264 SPS has the
//! picture size, the AAC AudioSpecificConfig has the sample rate and the
//! channel count. Nothing is decoded. This reads a few dozen bytes once per
//! publisher, when the sequence header arrives, and never again.

use crate::media_tag::{MediaTag, TagKind};

/// The video half of what a stream carries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Video {
    pub codec: String,
    pub width: u32,
    pub height: u32,
}

/// The audio half.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Audio {
    pub codec: String,
    pub channels: u32,
    pub sample_rate: u32,
}

/// Enhanced RTMP sets the top bit of the first video byte and puts a fourcc
/// after it.
fn enhanced(first: u8) -> bool {
    first & 0x80 != 0
}

/// The codec a video body names.
pub fn video_codec(body: &[u8]) -> String {
    let Some(&first) = body.first() else { return String::new() };
    if enhanced(first) {
        return match body.get(1..5) {
            Some(b"hvc1") => "h265".into(),
            Some(b"av01") => "av1".into(),
            Some(b"vp09") => "vp9".into(),
            Some(other) => String::from_utf8_lossy(other).into_owned(),
            None => "unknown".into(),
        };
    }
    match first & 0x0f {
        7 => "h264".into(),
        12 => "h265".into(),
        2 => "h263".into(),
        4 | 5 => "vp6".into(),
        other => format!("flv-video-{other}"),
    }
}

/// The codec an audio body names.
pub fn audio_codec(body: &[u8]) -> String {
    let Some(&first) = body.first() else { return String::new() };
    match first >> 4 {
        10 => "aac".into(),
        2 | 14 => "mp3".into(),
        11 => "speex".into(),
        7 | 8 => "g711".into(),
        other => format!("flv-audio-{other}"),
    }
}

/// Does this video body start a GOP?
pub fn is_keyframe(body: &[u8]) -> bool {
    body.first().map(|b| (b >> 4) & 0x07 == 1).unwrap_or(false)
}

/// Is this body a codec sequence header?
pub fn is_sequence_header(kind: TagKind, body: &[u8]) -> bool {
    let (Some(&first), Some(&second)) = (body.first(), body.get(1)) else { return false };
    match kind {
        TagKind::Video if enhanced(first) => first & 0x0f == 0,
        TagKind::Video => matches!(first & 0x0f, 7 | 12) && second == 0,
        TagKind::Audio => first >> 4 == 10 && second == 0,
        TagKind::Script => false,
    }
}

/// Read a video sequence header. H.264 and enhanced RTMP HEVC carry a size
/// this can read; the others (AV1 among them) report their codec and a size
/// of zero.
pub fn read_video(tag: &MediaTag) -> Video {
    let body = &tag.payload[..];
    let codec = video_codec(body);
    let (width, height) = if codec == "h264" && !enhanced(body[0]) {
        avc_size(body.get(5..).unwrap_or(&[])).unwrap_or((0, 0))
    } else if codec == "h265" && enhanced(body[0]) {
        crate::hevc::size_from_hvcc(body.get(5..).unwrap_or(&[])).unwrap_or((0, 0))
    } else {
        (0, 0)
    };
    Video { codec, width, height }
}

/// Read an audio sequence header.
pub fn read_audio(tag: &MediaTag) -> Audio {
    let body = &tag.payload[..];
    let codec = audio_codec(body);
    if codec != "aac" || body.len() < 4 {
        // The flags byte says 44.1 kHz stereo for everything but AAC, and is
        // right about it often enough to report.
        let rates = [5_512, 11_025, 22_050, 44_100];
        let first = body.first().copied().unwrap_or(0);
        return Audio {
            codec,
            channels: if first & 1 == 1 { 2 } else { 1 },
            sample_rate: rates[((first >> 2) & 3) as usize],
        };
    }
    // AudioSpecificConfig: five bits of object type, four of frequency index,
    // four of channel configuration.
    let config = u16::from_be_bytes([body[2], body[3]]);
    let index = ((config >> 7) & 0x0f) as usize;
    let rates = [
        96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025,
        8_000, 7_350,
    ];
    Audio {
        codec,
        channels: u32::from((config >> 3) & 0x0f),
        sample_rate: rates.get(index).copied().unwrap_or(0),
    }
}

/// The picture size out of an AVCDecoderConfigurationRecord's first SPS.
fn avc_size(record: &[u8]) -> Option<(u32, u32)> {
    let count = *record.get(5)? & 0x1f;
    if count == 0 {
        return None;
    }
    let len = u16::from_be_bytes([*record.get(6)?, *record.get(7)?]) as usize;
    let sps = record.get(8..8 + len)?;
    crate::sps::size(sps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn tag(kind: TagKind, body: &[u8]) -> MediaTag {
        MediaTag {
            kind,
            timestamp_ms: 0,
            keyframe: false,
            sequence_header: true,
            payload: Arc::from(body),
        }
    }

    #[test]
    fn the_first_byte_names_the_codec() {
        assert_eq!(video_codec(&[0x17, 0]), "h264");
        assert_eq!(video_codec(&[0x90, b'h', b'v', b'c', b'1']), "h265");
        assert_eq!(audio_codec(&[0xaf, 0]), "aac");
        assert_eq!(audio_codec(&[0x2f, 0]), "mp3");
    }

    #[test]
    fn a_sequence_header_and_a_keyframe_are_told_apart_from_the_rest() {
        assert!(is_sequence_header(TagKind::Video, &[0x17, 0x00]));
        assert!(!is_sequence_header(TagKind::Video, &[0x17, 0x01]));
        assert!(is_sequence_header(TagKind::Audio, &[0xaf, 0x00]));
        assert!(!is_sequence_header(TagKind::Audio, &[0xaf, 0x01]));
        assert!(is_keyframe(&[0x17, 0x01]));
        assert!(!is_keyframe(&[0x27, 0x01]));
        // Enhanced RTMP keeps the frame type in the same three bits.
        assert!(is_keyframe(&[0x91, b'h']));
    }

    #[test]
    fn an_aac_header_gives_its_rate_and_channels() {
        // AAC LC, 48 kHz, stereo: 00010 0011 0010 000 = 0x11 0x90.
        let audio = read_audio(&tag(TagKind::Audio, &[0xaf, 0x00, 0x11, 0x90]));
        assert_eq!(audio, Audio { codec: "aac".into(), channels: 2, sample_rate: 48_000 });
    }

    #[test]
    fn an_avc_header_gives_the_picture_size() {
        // A real 1280x720 High profile SPS, as x264 writes it.
        let sps = [
            0x67, 0x64, 0x00, 0x1f, 0xac, 0xd9, 0x40, 0x50, 0x05, 0xbb, 0x01, 0x10, 0x00, 0x00,
            0x03, 0x00, 0x10, 0x00, 0x00, 0x03, 0x03, 0xc0, 0xf1, 0x83, 0x19, 0x60,
        ];
        let mut body = vec![0x17, 0x00, 0, 0, 0, 1, 0x64, 0x00, 0x1f, 0xff, 0xe1];
        body.extend_from_slice(&(sps.len() as u16).to_be_bytes());
        body.extend_from_slice(&sps);
        let video = read_video(&tag(TagKind::Video, &body));
        assert_eq!(video, Video { codec: "h264".into(), width: 1280, height: 720 });
    }
}
