//! The picture size out of an H.264 sequence parameter set.
//!
//! Section 7.3.2.1.1 of the H.264 specification, read as far as the frame
//! cropping fields and no further. About forty bits of exponential Golomb
//! codes, read once per publisher.

/// A bit reader over an RBSP with the emulation prevention bytes taken out.
struct Bits {
    data: Vec<u8>,
    at: usize,
}

impl Bits {
    fn new(nal: &[u8]) -> Bits {
        // 00 00 03 is written wherever the payload would otherwise look like a
        // start code; the 03 is not part of the payload.
        let mut data = Vec::with_capacity(nal.len());
        let mut zeros = 0;
        for &b in nal {
            if zeros >= 2 && b == 3 {
                zeros = 0;
                continue;
            }
            zeros = if b == 0 { zeros + 1 } else { 0 };
            data.push(b);
        }
        Bits { data, at: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        let byte = *self.data.get(self.at / 8)?;
        let bit = (byte >> (7 - self.at % 8)) & 1;
        self.at += 1;
        Some(u32::from(bit))
    }

    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut out = 0;
        for _ in 0..n {
            out = (out << 1) | self.bit()?;
        }
        Some(out)
    }

    /// ue(v).
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1u32 << zeros) - 1 + self.bits(zeros)?)
    }

    /// se(v).
    fn se(&mut self) -> Option<i32> {
        let k = self.ue()? as i64;
        Some(if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) } as i32)
    }
}

/// Skip a scaling list, which only has to be walked to find what follows it.
fn skip_scaling_list(bits: &mut Bits, size: u32) -> Option<()> {
    let (mut last, mut next) = (8i32, 8i32);
    for _ in 0..size {
        if next != 0 {
            next = (last + bits.se()? + 256) % 256;
        }
        last = if next == 0 { last } else { next };
    }
    Some(())
}

/// The profiles whose SPS carries chroma format and bit depth fields.
fn high_profile(profile: u32) -> bool {
    matches!(profile, 100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135)
}

/// Width and height in pixels, after cropping. `nal` starts with the NAL
/// header byte.
pub fn size(nal: &[u8]) -> Option<(u32, u32)> {
    let mut b = Bits::new(nal.get(1..)?);
    let profile = b.bits(8)?;
    b.bits(16)?; // constraint flags and level
    b.ue()?; // seq_parameter_set_id
    let mut chroma = 1;
    if high_profile(profile) {
        chroma = b.ue()?;
        if chroma == 3 {
            b.bit()?; // separate_colour_plane_flag
        }
        b.ue()?; // bit_depth_luma
        b.ue()?; // bit_depth_chroma
        b.bit()?; // qpprime_y_zero_transform_bypass
        if b.bit()? == 1 {
            let lists = if chroma == 3 { 12 } else { 8 };
            for i in 0..lists {
                if b.bit()? == 1 {
                    skip_scaling_list(&mut b, if i < 6 { 16 } else { 64 })?;
                }
            }
        }
    }
    b.ue()?; // log2_max_frame_num
    match b.ue()? {
        0 => {
            b.ue()?;
        }
        1 => {
            b.bit()?;
            b.se()?;
            b.se()?;
            for _ in 0..b.ue()? {
                b.se()?;
            }
        }
        _ => {}
    }
    b.ue()?; // max_num_ref_frames
    b.bit()?; // gaps_in_frame_num_allowed
    let width_mbs = b.ue()? + 1;
    let height_units = b.ue()? + 1;
    let frame_mbs_only = b.bit()?;
    if frame_mbs_only == 0 {
        b.bit()?;
    }
    b.bit()?; // direct_8x8_inference
    let (mut left, mut right, mut top, mut bottom) = (0, 0, 0, 0);
    if b.bit()? == 1 {
        left = b.ue()?;
        right = b.ue()?;
        top = b.ue()?;
        bottom = b.ue()?;
    }
    let (crop_x, crop_y) = match chroma {
        0 => (1, 2 - frame_mbs_only),
        1 => (2, 2 * (2 - frame_mbs_only)),
        2 => (2, 2 - frame_mbs_only),
        _ => (1, 2 - frame_mbs_only),
    };
    let width = (width_mbs * 16).checked_sub((left + right) * crop_x)?;
    let height = (height_units * 16 * (2 - frame_mbs_only)).checked_sub((top + bottom) * crop_y)?;
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_golomb_reads_the_textbook_values() {
        // 1, 010, 011, 00100 are 0, 1, 2 and 3.
        let mut b = Bits { data: vec![0b1010_0110, 0b0100_0000], at: 0 };
        assert_eq!(b.ue(), Some(0));
        assert_eq!(b.ue(), Some(1));
        assert_eq!(b.ue(), Some(2));
        assert_eq!(b.ue(), Some(3));
    }

    #[test]
    fn an_emulation_prevention_byte_is_not_part_of_the_payload() {
        let b = Bits::new(&[0x00, 0x00, 0x03, 0x01]);
        assert_eq!(b.data, vec![0x00, 0x00, 0x01]);
    }

    #[test]
    fn a_1080p_sps_is_cropped_from_1088_to_1080() {
        // What x264 writes for 1920x1080: 68 macroblock rows, cropped by 8.
        let sps = [
            0x67, 0x64, 0x00, 0x28, 0xac, 0xd9, 0x40, 0x78, 0x02, 0x27, 0xe5, 0xc0, 0x44, 0x00,
            0x00, 0x03, 0x00, 0x04, 0x00, 0x00, 0x03, 0x00, 0xf0, 0x3c, 0x60, 0xc6, 0x58,
        ];
        assert_eq!(size(&sps), Some((1920, 1080)));
    }

    #[test]
    fn a_truncated_sps_is_nothing_rather_than_a_panic() {
        assert_eq!(size(&[0x67, 0x64]), None);
        assert_eq!(size(&[]), None);
    }
}
