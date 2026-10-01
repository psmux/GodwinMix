//! Enhanced RTMP audio bodies (E-RTMP v2): how a tag carries AC-3, E-AC-3
//! and MPEG audio layer II, none of which classic FLV has an id for.
//!
//! `MediaTag` carries FLV tag bodies, and its struct does not change for
//! these. Classic FLV names its audio codec in the top four bits of the
//! first byte (10 is AAC, 2 is MP3); E-RTMP v2 sets those bits to 9, puts a
//! packet type in the low four, and names the codec with a FourCC after it.
//! A reader that only knows classic FLV sees a sound format it does not
//! carry, which is what it would have seen anyway.
//!
//! AC-3 and E-AC-3 frames carry their own sync header, so no sequence start
//! is sent for them. Layer II goes under the specification's `.mp3` FourCC,
//! which names MPEG audio; the frame header says which layer, and
//! [`mpeg_audio`] reads it, so a stream of layer II is reported as `mp2`.

/// The sound format that says "enhanced, a FourCC follows".
pub const EX_HEADER: u8 = 9;
/// Packet types this plugin writes.
pub const SEQUENCE_START: u8 = 0;
pub const CODED_FRAMES: u8 = 1;

pub const AC3: &[u8; 4] = b"ac-3";
pub const EAC3: &[u8; 4] = b"ec-3";
pub const MPEG: &[u8; 4] = b".mp3";

/// The FourCC of an enhanced audio body, or `None` for a classic one.
pub fn fourcc(body: &[u8]) -> Option<[u8; 4]> {
    (*body.first()? >> 4 == EX_HEADER).then_some(())?;
    body.get(1..5)?.try_into().ok()
}

/// The five bytes in front of one coded frame.
pub fn prefix(cc: &[u8; 4]) -> [u8; 5] {
    [(EX_HEADER << 4) | CODED_FRAMES, cc[0], cc[1], cc[2], cc[3]]
}

/// What a frame header says: codec name, channels, sample rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub codec: &'static str,
    pub channels: u32,
    pub sample_rate: u32,
}

/// Read the frame after an enhanced audio prefix.
pub fn read(body: &[u8]) -> Option<Frame> {
    let frame = body.get(5..)?;
    match &fourcc(body)? {
        AC3 | EAC3 => ac3(frame),
        MPEG => mpeg_audio(frame),
        _ => None,
    }
}

/// The codec name an enhanced body gives, for [`crate::codec::audio_codec`].
pub fn codec(body: &[u8]) -> String {
    match fourcc(body) {
        Some(cc) if &cc == AC3 => "ac3".into(),
        Some(cc) if &cc == EAC3 => "eac3".into(),
        Some(cc) if &cc == MPEG => read(body).map_or("mp3", |f| f.codec).into(),
        Some(cc) if &cc == b"mp4a" => "aac".into(),
        Some(cc) if &cc == b"Opus" => "opus".into(),
        Some(cc) if &cc == b"fLaC" => "flac".into(),
        Some(cc) => String::from_utf8_lossy(&cc).into_owned(),
        None => "unknown".into(),
    }
}

/// An AC-3 or E-AC-3 sync frame header (ATSC A/52, sections 5.4 and E.1).
pub fn ac3(f: &[u8]) -> Option<Frame> {
    if f.get(..2)? != [0x0B, 0x77] {
        return None;
    }
    let bsid = *f.get(5)? >> 3;
    let base = [2u32, 1, 2, 3, 3, 4, 4, 5];
    if bsid > 10 {
        // E-AC-3: fscod(2) numblkscod(2) acmod(3) lfeon(1) in byte 4.
        let b = *f.get(4)?;
        let rate = [48_000, 44_100, 32_000, 24_000][usize::from(b >> 6)];
        let acmod = usize::from((b >> 1) & 7);
        return Some(Frame { codec: "eac3", channels: base[acmod] + u32::from(b & 1), sample_rate: rate });
    }
    let rate = [48_000, 44_100, 32_000, 0][usize::from(f[4] >> 6)];
    // acmod is the top three bits of byte 6, then up to three two bit
    // fields depending on it, then lfeon.
    let bits = u16::from_be_bytes([*f.get(6)?, *f.get(7)?]);
    let acmod = usize::from(bits >> 13);
    let mut at = 13;
    if acmod & 1 == 1 && acmod != 1 {
        at -= 2;
    }
    if acmod & 4 == 4 {
        at -= 2;
    }
    if acmod == 2 {
        at -= 2;
    }
    let lfe = u32::from((bits >> (at - 1)) & 1);
    Some(Frame { codec: "ac3", channels: base[acmod] + lfe, sample_rate: rate })
}

/// An MPEG audio frame header (ISO 11172-3): layer, rate and mode.
pub fn mpeg_audio(f: &[u8]) -> Option<Frame> {
    let (a, b, c, d) = (*f.first()?, *f.get(1)?, *f.get(2)?, *f.get(3)?);
    if a != 0xFF || b & 0xE0 != 0xE0 {
        return None;
    }
    let version = (b >> 3) & 3; // 3 is MPEG-1, 2 is MPEG-2, 0 is MPEG-2.5
    let codec = match (b >> 1) & 3 {
        3 => "mp1",
        2 => "mp2",
        1 => "mp3",
        _ => return None,
    };
    let base = [44_100, 48_000, 32_000, 0][usize::from((c >> 2) & 3)];
    let sample_rate = match version {
        3 => base,
        2 => base / 2,
        _ => base / 4,
    };
    let channels = if d >> 6 == 3 { 1 } else { 2 };
    Some(Frame { codec, channels, sample_rate })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(cc: &[u8; 4], frame: &[u8]) -> Vec<u8> {
        let mut b = prefix(cc).to_vec();
        b.extend_from_slice(frame);
        b
    }

    #[test]
    fn an_ac3_frame_says_five_point_one_at_48k() {
        // What ffmpeg's ac3 encoder writes for 5.1 at 48 kHz, 448 kbit/s:
        // fscod 0, bsid 8, acmod 7 with cmixlev and surmixlev, lfeon set.
        let frame = [0x0B, 0x77, 0x00, 0x00, 0x1C, 0x40, 0xE1, 0x7F, 0x00];
        let got = read(&body(AC3, &frame)).expect("an AC-3 header");
        assert_eq!(got, Frame { codec: "ac3", channels: 6, sample_rate: 48_000 });
        assert_eq!(codec(&body(AC3, &frame)), "ac3");
    }

    #[test]
    fn a_stereo_ac3_frame_has_no_lfe() {
        // acmod 2 has dsurmod after it, then lfeon clear.
        let frame = [0x0B, 0x77, 0, 0, 0x00, 0x40, 0x40, 0x00];
        assert_eq!(ac3(&frame).unwrap().channels, 2);
    }

    #[test]
    fn an_eac3_frame_is_told_apart_by_its_bsid() {
        let frame = [0x0B, 0x77, 0x00, 0xFF, 0x3F, 0x80];
        assert_eq!(ac3(&frame), Some(Frame { codec: "eac3", channels: 6, sample_rate: 48_000 }));
    }

    #[test]
    fn layer_two_and_layer_three_are_read_from_the_frame() {
        // MPEG-1 layer II, 48 kHz, stereo; then MPEG-1 layer III, 44.1 kHz, mono.
        assert_eq!(mpeg_audio(&[0xFF, 0xFD, 0x84, 0x00]), Some(Frame { codec: "mp2", channels: 2, sample_rate: 48_000 }));
        assert_eq!(mpeg_audio(&[0xFF, 0xFB, 0x90, 0xC0]), Some(Frame { codec: "mp3", channels: 1, sample_rate: 44_100 }));
        assert_eq!(codec(&body(MPEG, &[0xFF, 0xFD, 0x84, 0x00])), "mp2");
    }

    #[test]
    fn a_classic_body_has_no_fourcc() {
        assert_eq!(fourcc(&[0xAF, 1, 2, 3, 4]), None);
        assert_eq!(prefix(AC3), [0x91, b'a', b'c', b'-', b'3']);
    }
}
