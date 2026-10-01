//! The caps a decoder's appsrc is given: from a sequence header for AVC,
//! HEVC, AV1 and AAC, and from the frame itself for sound that has none.

use gstreamer as gst;

use crate::media_tag::{MediaTag, TagKind};

/// The caps a sequence header gives. None for a header this does not read,
/// and for any sound but AAC, whose caps come from its frames.
pub(crate) fn caps_for(header: &MediaTag) -> Option<gst::Caps> {
    let skip = if header.kind == TagKind::Audio { 2 } else { 5 };
    let config = gst::Buffer::from_slice(header.payload.get(skip..)?.to_vec());
    let caps = match (header.kind, crate::eflv::fourcc(&header.payload)) {
        (TagKind::Video, None) => {
            gst::Caps::builder("video/x-h264").field("stream-format", "avc").field("alignment", "au").field("codec_data", config)
        }
        // Enhanced RTMP: the configuration record is the one the codec's
        // parser takes as codec_data, hvcC for HEVC and av1C for AV1.
        (TagKind::Video, Some(cc)) if &cc == crate::eflv::HEVC => {
            gst::Caps::builder("video/x-h265").field("stream-format", "hvc1").field("alignment", "au").field("codec_data", config)
        }
        (TagKind::Video, Some(cc)) if &cc == crate::eflv::AV1 => {
            gst::Caps::builder("video/x-av1").field("stream-format", "obu-stream").field("alignment", "tu").field("codec_data", config)
        }
        (TagKind::Audio, _) if header.payload.first().is_some_and(|b| b >> 4 == 10) => {
            gst::Caps::builder("audio/mpeg").field("mpegversion", 4i32).field("stream-format", "raw").field("codec_data", config)
        }
        _ => return None,
    };
    Some(caps.build())
}

/// Sound that is not AAC, which carries no sequence header.
pub(crate) fn headerless(tag: &MediaTag) -> bool {
    tag.kind == TagKind::Audio && tag.payload.first().is_some_and(|b| b >> 4 != 10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn tag(kind: TagKind, body: &[u8]) -> MediaTag {
        MediaTag { kind, timestamp_ms: 0, keyframe: false, sequence_header: true, payload: Arc::from(body) }
    }

    #[test]
    fn only_an_aac_header_gives_aac_caps() {
        gst::init().unwrap();
        let aac = caps_for(&tag(TagKind::Audio, &[0xaf, 0, 0x11, 0x90])).unwrap();
        assert_eq!(aac.structure(0).unwrap().name(), "audio/mpeg");
        // A layer II frame under the enhanced `.mp3` FourCC is not AAC, and
        // AAC caps on it decode to nothing.
        let mp2 = tag(TagKind::Audio, &[0x91, b'.', b'm', b'p', b'3', 0xff, 0xfd]);
        assert!(caps_for(&mp2).is_none());
        assert!(headerless(&mp2));
        assert!(!headerless(&tag(TagKind::Audio, &[0xaf, 1, 0])));
    }
}
