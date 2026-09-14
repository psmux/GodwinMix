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

/// Tag types, from the FLV specification.
const TAG_AUDIO: u8 = 8;
const TAG_VIDEO: u8 = 9;

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
pub fn audio(timestamp_ms: u32, body: &[u8]) -> Vec<u8> {
    tag(TAG_AUDIO, timestamp_ms, body)
}

/// A video tag. `body` is the RTMP video message payload, unchanged.
pub fn video(timestamp_ms: u32, body: &[u8]) -> Vec<u8> {
    tag(TAG_VIDEO, timestamp_ms, body)
}

/// Is this video tag body a keyframe?
///
/// The top four bits of the first byte are the frame type, and 1 means a key
/// frame. Used to decide where a stream may safely be picked up: handing a
/// decoder a run of inter frames with no keyframe in front produces a grey
/// picture and a lot of log noise.
pub fn is_keyframe(body: &[u8]) -> bool {
    body.first().map(|b| b >> 4 == 1).unwrap_or(false)
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
    fn a_keyframe_is_recognised_and_an_inter_frame_is_not() {
        assert!(is_keyframe(&[0x17, 0x01]));
        assert!(!is_keyframe(&[0x27, 0x01]));
        assert!(!is_keyframe(&[]));
    }
}
