//! Writing FLV, which is the cheapest way to hand RTMP to the core.
//!
//! An RTMP audio or video message carries exactly the body of an FLV tag: the
//! same codec byte, the same AVC or AAC packet type, the same composition
//! offset. So turning a published RTMP stream into something the core can open
//! is a nine byte file header and an eleven byte header per message. Nothing is
//! parsed, nothing is re-timed, nothing is copied twice.
//!
//! The core's container transport is `fdsrc ! decodebin`, and `decodebin`
//! typefinds FLV and picks `flvdemux`. That is the same path the built in
//! `rtmp/source` takes after `rtmp2src`, so a stream that arrives here is
//! decoded by exactly the code that decodes one the core dialled out for.

use std::collections::HashMap;

use rml_rtmp::rml_amf0::{self, Amf0Value};
use rml_rtmp::sessions::StreamMetadata;

use crate::media_tag::{MediaTag, TagKind};

/// Tag types, from the FLV specification.
const TAG_AUDIO: u8 = 8;
const TAG_VIDEO: u8 = 9;
const TAG_SCRIPT: u8 = 18;

/// The nine byte file header plus the first "previous tag size" word.
///
/// The flags byte says which streams are present. Both bits are set because a
/// publisher may start with either and add the other; `flvdemux` copes, and
/// claiming audio that never comes is better than claiming none and having it
/// arrive.
pub fn header() -> Vec<u8> {
    let mut out = Vec::with_capacity(13);
    out.extend_from_slice(b"FLV");
    out.push(1); // version
    out.push(0b0000_0101); // audio and video present
    out.extend_from_slice(&9u32.to_be_bytes()); // header size
    out.extend_from_slice(&0u32.to_be_bytes()); // previous tag size
    out
}

/// One tag: the eleven byte header, the body, and the trailing size word.
fn tag(kind: u8, timestamp_ms: u32, body: &[u8]) -> Vec<u8> {
    let size = body.len() as u32;
    let mut out = Vec::with_capacity(15 + body.len());
    out.push(kind);
    out.extend_from_slice(&size.to_be_bytes()[1..4]);
    // The timestamp is a 24 bit field with its top byte held separately, which
    // is how FLV reaches beyond four and a half hours.
    out.extend_from_slice(&timestamp_ms.to_be_bytes()[1..4]);
    out.push(timestamp_ms.to_be_bytes()[0]);
    out.extend_from_slice(&[0, 0, 0]); // stream id, always zero
    out.extend_from_slice(body);
    out.extend_from_slice(&(11 + size).to_be_bytes());
    out
}

/// An audio tag. `body` is the RTMP audio message payload, unchanged.
#[cfg(test)]
pub fn audio(timestamp_ms: u32, body: &[u8]) -> Vec<u8> {
    tag(TAG_AUDIO, timestamp_ms, body)
}

/// A video tag. `body` is the RTMP video message payload, unchanged.
#[cfg(test)]
pub fn video(timestamp_ms: u32, body: &[u8]) -> Vec<u8> {
    tag(TAG_VIDEO, timestamp_ms, body)
}

/// Any tag the hub hands on, as FLV bytes.
pub fn write(tag: &MediaTag) -> Vec<u8> {
    self::tag(kind_byte(tag.kind), tag.timestamp_ms, &tag.payload)
}

/// The same, straight into a writer, so the payload is never copied into a
/// buffer of its own on the way to a socket.
pub fn write_to(out: &mut impl std::io::Write, tag: &MediaTag) -> std::io::Result<()> {
    let size = tag.payload.len() as u32;
    let ts = tag.timestamp_ms.to_be_bytes();
    let s = size.to_be_bytes();
    let head = [kind_byte(tag.kind), s[1], s[2], s[3], ts[1], ts[2], ts[3], ts[0], 0, 0, 0];
    out.write_all(&head)?;
    out.write_all(&tag.payload)?;
    out.write_all(&(11 + size).to_be_bytes())
}

fn kind_byte(kind: TagKind) -> u8 {
    match kind {
        TagKind::Audio => TAG_AUDIO,
        TagKind::Video => TAG_VIDEO,
        TagKind::Script => TAG_SCRIPT,
    }
}

/// The body of an `onMetaData` script tag, rebuilt from what the publisher
/// said in its `@setDataFrame`.
///
/// `rml_rtmp` parses the publisher's metadata into a struct and does not keep
/// the bytes, so they are written again here with the AMF0 encoder that crate
/// already carries. A late reader, and a restream to a platform that shows
/// the resolution it was told, both want it.
pub fn metadata_body(meta: &StreamMetadata) -> Vec<u8> {
    let mut props: HashMap<String, Amf0Value> = HashMap::new();
    let mut number = |name: &str, value: Option<f64>| {
        if let Some(v) = value {
            props.insert(name.to_string(), Amf0Value::Number(v));
        }
    };
    number("width", meta.video_width.map(f64::from));
    number("height", meta.video_height.map(f64::from));
    number("videocodecid", meta.video_codec_id.map(f64::from));
    number("framerate", meta.video_frame_rate.map(f64::from));
    number("videodatarate", meta.video_bitrate_kbps.map(f64::from));
    number("audiocodecid", meta.audio_codec_id.map(f64::from));
    number("audiodatarate", meta.audio_bitrate_kbps.map(f64::from));
    number("audiosamplerate", meta.audio_sample_rate.map(f64::from));
    number("audiochannels", meta.audio_channels.map(f64::from));
    if let Some(stereo) = meta.audio_is_stereo {
        props.insert("stereo".into(), Amf0Value::Boolean(stereo));
    }
    if let Some(encoder) = &meta.encoder {
        props.insert("encoder".into(), Amf0Value::Utf8String(encoder.clone()));
    }
    let values = vec![Amf0Value::Utf8String("onMetaData".into()), Amf0Value::Object(props)];
    rml_amf0::serialize(&values).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_is_the_thirteen_bytes_flvdemux_looks_for() {
        let h = header();
        assert_eq!(h.len(), 13);
        assert_eq!(&h[0..3], b"FLV");
        assert_eq!(h[3], 1);
        assert_eq!(u32::from_be_bytes([h[5], h[6], h[7], h[8]]), 9);
        assert_eq!(u32::from_be_bytes([h[9], h[10], h[11], h[12]]), 0);
    }

    #[test]
    fn a_tag_carries_its_body_unchanged_and_says_how_long_it_was() {
        let body = [0x17u8, 0x00, 0x00, 0x00, 0x00, 0x42];
        let t = video(0, &body);
        assert_eq!(t.len(), 11 + body.len() + 4);
        assert_eq!(t[0], TAG_VIDEO);
        assert_eq!(u32::from_be_bytes([0, t[1], t[2], t[3]]), body.len() as u32);
        assert_eq!(&t[11..11 + body.len()], &body);
        let trailer = u32::from_be_bytes([t[15 + body.len() - 4], t[16 + body.len() - 4],
                                          t[17 + body.len() - 4], t[18 + body.len() - 4]]);
        assert_eq!(trailer, 11 + body.len() as u32);
    }

    #[test]
    fn an_audio_tag_is_marked_as_audio() {
        assert_eq!(audio(0, &[0xaf, 0x00])[0], TAG_AUDIO);
    }

    #[test]
    fn a_timestamp_beyond_twenty_four_bits_keeps_its_top_byte() {
        // 0x01_02_03_04 milliseconds: the low three bytes go in the 24 bit
        // field, the top one in the extension byte after it.
        let t = video(0x0102_0304, &[0x17]);
        assert_eq!(&t[4..7], &[0x02, 0x03, 0x04]);
        assert_eq!(t[7], 0x01);
    }

    #[test]
    fn metadata_becomes_an_on_metadata_script_body() {
        let mut meta = StreamMetadata::new();
        meta.video_width = Some(1920);
        meta.video_frame_rate = Some(30.0);
        let body = metadata_body(&meta);
        let values = rml_amf0::deserialize(&mut std::io::Cursor::new(body)).expect("AMF0");
        assert_eq!(values[0], Amf0Value::Utf8String("onMetaData".into()));
        let Amf0Value::Object(props) = &values[1] else { panic!("{values:?}") };
        assert_eq!(props["width"], Amf0Value::Number(1920.0));
        let t = write(&MediaTag {
            kind: TagKind::Script,
            timestamp_ms: 0,
            keyframe: false,
            sequence_header: false,
            payload: std::sync::Arc::from(&b"x"[..]),
        });
        assert_eq!(t[0], TAG_SCRIPT);
    }

    #[test]
    fn writing_into_a_writer_gives_the_same_bytes_as_building_the_tag() {
        let tag = MediaTag {
            kind: TagKind::Video,
            timestamp_ms: 0x0102_0304,
            keyframe: true,
            sequence_header: false,
            payload: std::sync::Arc::from(&[0x17u8, 1, 0, 0, 0, 9][..]),
        };
        let mut out = Vec::new();
        write_to(&mut out, &tag).unwrap();
        assert_eq!(out, write(&tag));
        assert_eq!(out, video(0x0102_0304, &tag.payload));
    }
}
