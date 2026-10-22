//! Which H.264 access units a decoder can start at, as `h264parse` decides
//! it: an IDR, or an I slice behind a recovery point SEI. A broadcast
//! encoder that codes fields and never sends an IDR starts its GOPs the
//! second way, and a reader waiting for a keyframe must be given one.

const IDR: u8 = 5;
const SEI: u8 = 6;
const SLICE: u8 = 1;
const SPS: u8 = 7;
const PPS: u8 = 8;

/// What one access unit holds that says whether a decoder can start at it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    pub idr: bool,
    /// A recovery point SEI (payload type 6).
    pub recovery: bool,
    /// The first slice is an I or SI slice (slice_type 2, 4, 7 or 9).
    pub intra: bool,
    pub sps: bool,
    pub pps: bool,
}

impl Seen {
    pub fn nal(&mut self, nal: &[u8]) {
        match nal[0] & 0x1F {
            IDR => self.idr = true,
            SPS => self.sps = true,
            PPS => self.pps = true,
            SEI => self.recovery |= recovery_point(&unescape(&nal[1..])),
            SLICE if !self.intra => self.intra = matches!(slice_type(&nal[1..]), Some(2 | 4 | 7 | 9)),
            _ => {}
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

/// `slice_type` from a slice header (7.3.3), after its NAL header byte.
fn slice_type(b: &[u8]) -> Option<u32> {
    let mut bits = crate::sps::Bits::new(b.get(..8.min(b.len()))?);
    bits.ue()?;
    bits.ue()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(nals: &[&[u8]]) -> Seen {
        let mut s = Seen::default();
        nals.iter().for_each(|n| s.nal(n));
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
}
