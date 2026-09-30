//! Enhanced RTMP video bodies (the E-RTMP v1 specification): how a tag says
//! it is HEVC or AV1, where its frame starts, and how to write one.
//!
//! The first byte has its top bit set, then the frame type in three bits and
//! the packet type in four; a FourCC names the codec. A sequence start
//! carries the codec's configuration record (`hvcC`, `av1C`); coded frames
//! carry the frame, with a 24 bit composition offset first for HEVC only.

/// Packet types this plugin reads and writes.
pub const SEQUENCE_START: u8 = 0;
pub const CODED_FRAMES: u8 = 1;
pub const CODED_FRAMES_X: u8 = 3;

pub const HEVC: &[u8; 4] = b"hvc1";
pub const AV1: &[u8; 4] = b"av01";

/// The FourCC of an enhanced body, or `None` for a classic one.
pub fn fourcc(body: &[u8]) -> Option<[u8; 4]> {
    let first = *body.first()?;
    if first & 0x80 == 0 {
        return None;
    }
    body.get(1..5)?.try_into().ok()
}

/// Where the frame or the configuration record starts, and the composition
/// offset in ms, for a video body of either kind.
pub fn frame(body: &[u8]) -> Option<(usize, i32)> {
    match fourcc(body) {
        None => {
            let b = body.get(2..5)?;
            Some((5, ((i32::from(b[0]) << 16) | (i32::from(b[1]) << 8) | i32::from(b[2])) << 8 >> 8))
        }
        Some(cc) => {
            let packet = body[0] & 0x0f;
            if packet == CODED_FRAMES && &cc == HEVC {
                let b = body.get(5..8)?;
                return Some((8, ((i32::from(b[0]) << 16) | (i32::from(b[1]) << 8) | i32::from(b[2])) << 8 >> 8));
            }
            matches!(packet, SEQUENCE_START | CODED_FRAMES | CODED_FRAMES_X).then_some((5, 0))
        }
    }
}

/// The bytes in front of an HEVC or AV1 frame or configuration record.
pub fn prefix(cc: &[u8; 4], keyframe: bool, header: bool, cts_ms: i64) -> Vec<u8> {
    let frame_type: u8 = if keyframe || header { 1 } else { 2 };
    let hevc_cts = cc == HEVC && !header && cts_ms != 0;
    let packet = match (header, hevc_cts) {
        (true, _) => SEQUENCE_START,
        (false, true) => CODED_FRAMES,
        (false, false) if cc == HEVC => CODED_FRAMES_X,
        (false, false) => CODED_FRAMES,
    };
    let mut out = vec![0x80 | (frame_type << 4) | packet];
    out.extend_from_slice(cc);
    if hevc_cts {
        let cts = (cts_ms.clamp(-(1 << 23), (1 << 23) - 1) as i32).to_be_bytes();
        out.extend_from_slice(&cts[1..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_body_reads_back() {
        let head = prefix(HEVC, true, true, 0);
        assert_eq!(head, [0x90, b'h', b'v', b'c', b'1']);
        assert_eq!(fourcc(&head), Some(*HEVC));
        let with_cts = prefix(HEVC, false, false, 40);
        assert_eq!(with_cts[0], 0xa1);
        assert_eq!(frame(&with_cts), Some((8, 40)));
        assert_eq!(frame(&prefix(HEVC, true, false, 0)), Some((5, 0)));
        assert_eq!(prefix(AV1, true, false, 0)[0], 0x91, "AV1 frames carry no offset");
        // A classic AVC body: 0x17, packet type 1, a 24 bit offset of -1.
        assert_eq!(frame(&[0x17, 1, 0xff, 0xff, 0xff, 0]), Some((5, -1)));
        assert_eq!(fourcc(&[0x17, 1]), None);
    }
}
