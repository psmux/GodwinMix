//! Which H.264 access units a decoder can start at, as `h264parse` decides
//! it: an IDR, or an I slice behind a recovery point SEI. A broadcast
//! encoder that codes fields and never sends an IDR starts its GOPs the
//! second way, and a reader waiting for a keyframe must be given one.
//!
//! The first slice also says whether the access unit is a field, which is
//! how the frame rate of a field coded stream is told from its field rate.

use crate::sps::{Bits, Layout};

const IDR: u8 = 5;
const SEI: u8 = 6;
const SLICE: u8 = 1;

/// What one access unit holds that says whether a decoder can start at it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    pub idr: bool,
    /// A recovery point SEI (payload type 6).
    pub recovery: bool,
    /// The first slice is an I or SI slice (slice_type 2, 4, 7 or 9).
    pub intra: bool,
    /// The first slice is of a field, not a frame (`field_pic_flag`).
    pub field: bool,
    /// The first slice has been read.
    sliced: bool,
}

impl Seen {
    /// One NAL unit, read with the stream's SPS when there is one.
    pub fn nal(&mut self, nal: &[u8], sps: Option<&Layout>) {
        let kind = nal[0] & 0x1F;
        self.idr |= kind == IDR;
        if kind == SEI {
            self.recovery |= recovery_point(&unescape(&nal[1..]));
        }
        if (kind == SLICE || kind == IDR) && !std::mem::replace(&mut self.sliced, true) {
            if let Some((slice_type, field)) = slice(&nal[1..], sps) {
                (self.intra, self.field) = (matches!(slice_type, 2 | 4 | 7 | 9), field);
            }
        }
    }

    /// A keyframe as `h264parse` marks one: an IDR, or an I slice behind a
    /// recovery point, which is where a broadcast encoder that never sends
    /// an IDR lets a decoder start.
    pub fn keyframe(&self) -> bool {
        self.idr || (self.recovery && self.intra)
    }
}

/// Whether an SEI NAL (after its header byte) carries a recovery point.
fn recovery_point(mut b: &[u8]) -> bool {
    while b.len() >= 2 && b[0] != 0x80 {
        let mut kind = 0usize;
        while let Some((&x, rest)) = b.split_first() {
            kind += usize::from(x);
            b = rest;
            if x != 0xFF {
                break;
            }
        }
        let mut size = 0usize;
        while let Some((&x, rest)) = b.split_first() {
            size += usize::from(x);
            b = rest;
            if x != 0xFF {
                break;
            }
        }
        if kind == 6 {
            return true;
        }
        b = b.get(size..).unwrap_or_default();
    }
    false
}

/// The RBSP: `00 00 03` written wherever the payload would look like a
/// start code, the `03` taken out.
fn unescape(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    for (i, &x) in b.iter().enumerate() {
        if !(x == 3 && i >= 2 && b[i - 1] == 0 && b[i - 2] == 0 && out.ends_with(&[0, 0])) {
            out.push(x);
        }
    }
    out
}

/// `slice_type`, and whether the picture is a field, from a slice header
/// (7.3.3) after its NAL header byte. Without the SPS it is taken as a
/// frame.
fn slice(b: &[u8], sps: Option<&Layout>) -> Option<(u32, bool)> {
    let mut bits = Bits::new(b.get(..b.len().min(24))?);
    bits.ue()?;
    let slice_type = bits.ue()?;
    let field = sps.and_then(|l| {
        bits.ue()?;
        if l.separate_colour_plane {
            bits.bits(2)?;
        }
        bits.bits(l.frame_num_bits)?;
        Some(!l.frame_mbs_only && bits.bit()? == 1)
    });
    Some((slice_type, field.unwrap_or(false)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(nals: &[&[u8]]) -> Seen {
        let mut s = Seen::default();
        nals.iter().for_each(|n| s.nal(n, None));
        s
    }

    #[test]
    fn an_i_slice_behind_a_recovery_point_is_a_keyframe_and_a_p_slice_is_not() {
        // SEI: payload type 6, one byte, then the trailing bits.
        let recovery: &[u8] = &[0x06, 0x06, 0x01, 0x84, 0x80];
        // Non-IDR slices: first_mb 0, then slice_type 7 (I) or 5 (P).
        let (i, p): (&[u8], &[u8]) = (&[0x21, 0x88, 0x80], &[0x21, 0x98, 0x80]);
        assert!(seen(&[recovery, i]).keyframe());
        assert!(!seen(&[recovery, p]).keyframe(), "a recovery point on a P slice");
        assert!(!seen(&[i]).keyframe(), "an I slice with no recovery point");
        assert!(seen(&[&[0x65, 0x88]]).keyframe(), "an IDR");
        // A user data SEI (type 5) ahead of the recovery point in one NAL.
        let both: &[u8] = &[0x06, 0x05, 0x01, 0xAA, 0x06, 0x01, 0x84, 0x80];
        assert!(seen(&[both, i]).keyframe());
    }

    #[test]
    fn field_pic_flag_says_a_slice_is_a_field() {
        let sps = Layout { width: 640, height: 480, separate_colour_plane: false, frame_num_bits: 4, frame_mbs_only: false };
        // first_mb 0, slice_type 7, pps 0, frame_num 0000, then the flag.
        let (field, frame): (&[u8], &[u8]) = (&[0x21, 0x88, 0x86], &[0x21, 0x88, 0x82]);
        let mut s = Seen::default();
        s.nal(field, Some(&sps));
        assert!(s.field && s.intra);
        let mut s = Seen::default();
        s.nal(frame, Some(&sps));
        assert!(!s.field);
        let progressive = Layout { frame_mbs_only: true, ..sps };
        let mut s = Seen::default();
        s.nal(field, Some(&progressive));
        assert!(!s.field, "a stream of frames only has no flag to read");
    }
}
