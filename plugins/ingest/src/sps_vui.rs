//! The frame rate an H.264 SPS declares, from the timing in its VUI.
//!
//! Annex E.1.1 of the H.264 specification: the VUI follows the frame
//! cropping fields, and its timing gives `time_scale` ticks a second with
//! two ticks to a frame, so 60000 and 1001 is 29.97 frames a second whether
//! the frames are coded as frames or as fields.

use super::Bits;

/// The frame rate the SPS declares, or none when it declares no timing.
/// `nal` starts with the NAL header byte.
pub fn frame_rate(nal: &[u8]) -> Option<f64> {
    let (_, mut b) = super::walk(nal)?;
    if b.bit()? == 0 {
        return None; // no VUI
    }
    if b.bit()? == 1 && b.bits(8)? == 255 {
        b.bits(32)?; // aspect_ratio_idc, then an extended sample aspect ratio
    }
    if b.bit()? == 1 {
        b.bit()?; // overscan_appropriate
    }
    if b.bit()? == 1 {
        b.bits(4)?; // video_format and full range
        if b.bit()? == 1 {
            b.bits(24)?; // colour primaries, transfer, matrix
        }
    }
    if b.bit()? == 1 {
        b.ue()?; // chroma sample location, top and bottom field
        b.ue()?;
    }
    if b.bit()? == 0 {
        return None; // no timing
    }
    let (units, scale) = (b.bits(32)?, b.bits(32)?);
    let fps = f64::from(scale) / (2.0 * f64::from(units));
    (units > 0 && (1.0..=300.0).contains(&fps)).then_some(fps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x264_s_timing_is_read_as_frames_a_second() {
        // 1280x720 at 60000/1001, High.
        let p720 = [
            0x67, 0x64, 0x00, 0x20, 0xac, 0xd9, 0x40, 0x50, 0x05, 0xbb, 0x01, 0x10, 0x00, 0x00, 0x3e, 0x90, 0x00, 0x1d,
            0x4c, 0x00, 0xf1, 0x83, 0x19, 0x60,
        ];
        assert!((frame_rate(&p720).unwrap() - 59.94).abs() < 0.01, "{:?}", frame_rate(&p720));
        // 1920x1080 coded as fields at 30000/1001: the rate is the frames'.
        let i1080 = [
            0x67, 0x64, 0x00, 0x28, 0xac, 0xd9, 0x40, 0x78, 0x04, 0x4f, 0xde, 0x02, 0x20, 0x00, 0x00, 0x7d, 0x20, 0x00,
            0x1d, 0x4c, 0x03, 0xe2, 0xc5, 0xb2, 0xc0,
        ];
        assert_eq!(super::super::size(&i1080), Some((1920, 1080)));
        assert!((frame_rate(&i1080).unwrap() - 29.97).abs() < 0.01, "{:?}", frame_rate(&i1080));
        // 640x360 at 25, Constrained Baseline, with an emulation prevention byte in its timing.
        let p360 = [
            0x67, 0x42, 0xc0, 0x1e, 0xda, 0x02, 0x80, 0xbf, 0xe5, 0xc0, 0x44, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00, 0x00,
            0x03, 0x00, 0xc8, 0x3c, 0x58, 0xba, 0x80,
        ];
        assert_eq!(frame_rate(&p360), Some(25.0));
    }

    #[test]
    fn an_sps_that_stops_short_declares_nothing() {
        let short = [0x67, 0x42, 0xc0, 0x1e, 0xda, 0x02, 0x80, 0xbf, 0xe5];
        assert_eq!(frame_rate(&short), None);
        assert_eq!(frame_rate(&[]), None);
    }
}
