//! What an FLV tag from the hub is, as the packager takes it: the caps its
//! sequence header gives, and where the frame starts.
//!
//! Video is AVC as classic FLV frames it, or HEVC and AV1 as enhanced RTMP
//! does (`plugins/ingest/src/eflv.rs`). Sound is AAC as classic FLV frames
//! it; AC-3, E-AC-3 and MPEG audio arrive as enhanced bodies or as MP3, and
//! fragmented MP4 here carries none of them, so [`audio`] names them for a
//! refusal instead.

use gstreamer as gst;

/// What one tag is.
#[derive(Debug, Clone, PartialEq)]
pub enum Read {
    /// A configuration record: the caps every frame after it has.
    Header(gst::Caps),
    /// A frame: where it starts in the body, its composition offset in ms,
    /// and whether it is a keyframe.
    Frame { skip: usize, cts: i32, key: bool },
    /// Sound in a codec fragmented MP4 is not given here: its name.
    Unsupported(String),
    /// Nothing to package: an end of sequence, metadata.
    Skip,
}

fn cts(b: &[u8]) -> i32 {
    ((i32::from(b[0]) << 16) | (i32::from(b[1]) << 8) | i32::from(b[2])) << 8 >> 8
}

fn config(body: &[u8], skip: usize) -> Option<gst::Buffer> {
    Some(gst::Buffer::from_slice(body.get(skip..)?.to_vec()))
}

/// A video body.
pub fn video(body: &[u8]) -> Read {
    let Some(&first) = body.first() else { return Read::Skip };
    if first & 0x80 == 0 {
        return classic_video(body, first);
    }
    let (Some(cc), packet, key) = (body.get(1..5), first & 0x0f, (first >> 4) & 0x07 == 1) else { return Read::Skip };
    let (name, format, align) = match cc {
        b"hvc1" => ("video/x-h265", "hvc1", "au"),
        b"av01" => ("video/x-av1", "obu-stream", "tu"),
        other => return Read::Unsupported(String::from_utf8_lossy(other).into_owned()),
    };
    match packet {
        0 => match config(body, 5) {
            Some(c) => Read::Header(gst::Caps::builder(name).field("stream-format", format).field("alignment", align).field("codec_data", c).build()),
            None => Read::Skip,
        },
        1 if cc == b"hvc1" && body.len() > 8 => Read::Frame { skip: 8, cts: cts(&body[5..8]), key },
        1 | 3 if body.len() > 5 => Read::Frame { skip: 5, cts: 0, key },
        _ => Read::Skip,
    }
}

fn classic_video(body: &[u8], first: u8) -> Read {
    if first & 0x0f != 7 {
        return Read::Unsupported(format!("FLV video codec {}", first & 0x0f));
    }
    match body.get(1) {
        Some(0) => match config(body, 5) {
            Some(c) => Read::Header(gst::Caps::builder("video/x-h264").field("stream-format", "avc").field("alignment", "au").field("codec_data", c).build()),
            None => Read::Skip,
        },
        Some(1) if body.len() > 5 => Read::Frame { skip: 5, cts: cts(&body[2..5]), key: first >> 4 == 1 },
        _ => Read::Skip,
    }
}

/// A sound body.
pub fn audio(body: &[u8]) -> Read {
    let Some(&first) = body.first() else { return Read::Skip };
    match first >> 4 {
        10 => match body.get(1) {
            Some(0) => match config(body, 2) {
                Some(c) => Read::Header(gst::Caps::builder("audio/mpeg").field("mpegversion", 4i32).field("stream-format", "raw").field("codec_data", c).build()),
                None => Read::Skip,
            },
            Some(1) if body.len() > 2 => Read::Frame { skip: 2, cts: 0, key: true },
            _ => Read::Skip,
        },
        2 | 14 => Read::Unsupported("mp3".into()),
        9 => Read::Unsupported(match body.get(1..5) {
            Some(b"ac-3") => "ac3".into(),
            Some(b"ec-3") => "eac3".into(),
            Some(b".mp3") => "mpeg audio (mp2 or mp3)".into(),
            Some(other) => String::from_utf8_lossy(other).into_owned(),
            None => "unknown".into(),
        }),
        other => Read::Unsupported(format!("FLV sound format {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(r: &Read) -> String {
        match r {
            Read::Header(c) => c.structure(0).unwrap().name().to_string(),
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn avc_hevc_and_aac_are_read_and_other_sound_is_named() {
        gst::init().unwrap();
        assert_eq!(name(&video(&[0x17, 0, 0, 0, 0, 1, 2])), "video/x-h264");
        assert_eq!(video(&[0x17, 1, 0xff, 0xff, 0xff, 9]), Read::Frame { skip: 5, cts: -1, key: true });
        assert_eq!(video(&[0x27, 1, 0, 0, 40, 9]), Read::Frame { skip: 5, cts: 40, key: false });
        assert_eq!(name(&video(&[0x90, b'h', b'v', b'c', b'1', 1])), "video/x-h265");
        assert_eq!(video(&[0xa1, b'h', b'v', b'c', b'1', 0, 0, 40, 9]), Read::Frame { skip: 8, cts: 40, key: false });
        assert_eq!(video(&[0x93, b'h', b'v', b'c', b'1', 9]), Read::Frame { skip: 5, cts: 0, key: true });
        assert_eq!(name(&audio(&[0xaf, 0, 0x12, 0x10])), "audio/mpeg");
        assert_eq!(audio(&[0xaf, 1, 7]), Read::Frame { skip: 2, cts: 0, key: true });
        assert_eq!(audio(&[0x91, b'a', b'c', b'-', b'3', 0x0b, 0x77]), Read::Unsupported("ac3".into()));
        assert!(matches!(audio(&[0x91, b'.', b'm', b'p', b'3', 0xff]), Read::Unsupported(s) if s.contains("mp2")));
        assert_eq!(audio(&[0x2f, 0xff]), Read::Unsupported("mp3".into()));
    }
}
