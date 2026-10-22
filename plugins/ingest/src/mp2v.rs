//! MPEG-2 video as a hub tag. Nothing on a wire carries MPEG-2 in FLV, so
//! the FourCC `mp2v` is ours: a tag body of this kind is only ever read by
//! this process's own transcoder, and a copy output that meets one says it
//! needs a rendition.
//!
//! ```text
//!   byte 0     0x80 | frame type << 4 | 1   (1 keyframe, 2 other; packet type coded frames)
//!   bytes 1..5 b"mp2v"
//!   bytes 5..8 composition offset, ms, signed 24 bit big endian (PTS less DTS)
//!   bytes 8..  one coded picture as it was in the elementary stream: any
//!              sequence header, sequence extension, GOP header and user data
//!              in front of it, the picture header, its extensions and slices,
//!              up to the next picture's first start code
//! ```
//!
//! The tag's `timestamp_ms` is the picture's decode time and its keyframe flag
//! is set when the picture has a sequence header in front of it, which is
//! where a decoder can start. There is no sequence start tag: as with AC-3,
//! each keyframe says what the stream is, and [`sequence`] reads it.

use std::sync::Arc;

use crate::media_tag::{MediaTag, TagKind};

pub const FOURCC: &[u8; 4] = b"mp2v";

/// Bytes in front of the picture.
pub const PREFIX: usize = 8;

const SEQUENCE_HEADER: u8 = 0xb3;
const EXTENSION: u8 = 0xb5;

/// One picture as a tag of track 0.
pub fn tag(picture: &[u8], dts_ms: u32, cts_ms: i32) -> MediaTag {
    let keyframe = starts_gop(picture);
    let mut payload = Vec::with_capacity(PREFIX + picture.len());
    let frame_type: u8 = if keyframe { 1 } else { 2 };
    payload.push(0x80 | (frame_type << 4) | crate::eflv::CODED_FRAMES);
    payload.extend_from_slice(FOURCC);
    let cts = cts_ms.clamp(-(1 << 23), (1 << 23) - 1).to_be_bytes();
    payload.extend_from_slice(&cts[1..]);
    payload.extend_from_slice(picture);
    MediaTag { track: 0, kind: TagKind::Video, timestamp_ms: dts_ms, keyframe, sequence_header: false, payload: Arc::from(payload) }
}

/// Whether a tag body is MPEG-2 video.
pub fn is(body: &[u8]) -> bool {
    crate::eflv::fourcc(body).is_some_and(|cc| &cc == FOURCC)
}

/// Whether a picture has a sequence header in front of it.
pub fn starts_gop(picture: &[u8]) -> bool {
    codes(picture).any(|(code, _)| code == SEQUENCE_HEADER)
}

/// Every start code in `data`: the byte after `00 00 01` and where the
/// header after it begins.
fn codes(data: &[u8]) -> impl Iterator<Item = (u8, usize)> + '_ {
    let mut at = 0;
    std::iter::from_fn(move || {
        while at + 3 < data.len() {
            let i = at;
            at += 1;
            if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
                at = i + 4;
                return Some((data[i + 3], i + 4));
            }
        }
        None
    })
}

/// What a sequence header and its extension say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequence {
    pub width: u32,
    pub height: u32,
    /// The frame rate as a fraction: 30000/1001, 60000/1001, 25/1.
    pub fps: (u32, u32),
    /// False for an interlaced stream, or for MPEG-1, which has no extension.
    pub progressive: bool,
}

/// The sequence in front of a keyframe, or `None` on any other picture.
pub fn sequence(picture: &[u8]) -> Option<Sequence> {
    let (_, at) = codes(picture).find(|(c, _)| *c == SEQUENCE_HEADER)?;
    let h = picture.get(at..at + 4)?;
    let mut width = (u32::from(h[0]) << 4) | (u32::from(h[1]) >> 4);
    let mut height = ((u32::from(h[1]) & 0x0f) << 8) | u32::from(h[2]);
    let (mut num, mut den) = match h[3] & 0x0f {
        1 => (24000, 1001),
        2 => (24, 1),
        3 => (25, 1),
        4 => (30000, 1001),
        5 => (30, 1),
        6 => (50, 1),
        7 => (60000, 1001),
        8 => (60, 1),
        _ => return None,
    };
    let mut progressive = false;
    // The sequence extension: id 1 in the top four bits.
    let ext = codes(&picture[at..]).find(|(c, i)| *c == EXTENSION && picture.get(at + i).is_some_and(|b| b >> 4 == 1));
    if let Some(e) = ext.and_then(|(_, i)| picture.get(at + i..at + i + 6)) {
        progressive = e[1] & 0x08 != 0;
        width |= ((u32::from(e[1]) & 0x01) << 13) | ((u32::from(e[2]) & 0x80) << 5);
        height |= (u32::from(e[2]) & 0x60) << 7;
        let (n, d) = (u32::from((e[5] >> 5) & 0x03) + 1, u32::from(e[5] & 0x1f) + 1);
        (num, den) = (num * n, den * d);
    }
    Some(Sequence { width, height, fps: (num, den), progressive })
}

#[cfg(test)]
#[path = "mp2v_tests.rs"]
mod tests;
