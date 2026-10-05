//! The picture size out of an HEVC decoder configuration record, for a
//! channel stream published as enhanced RTMP HEVC or as HEVC in MPEG-TS.
//!
//! Section 7.3.2.2 of H.265, read as far as the conformance window and no
//! further: the profile, tier and level block is skipped by its known length,
//! then two exponential Golomb codes give the size.

use crate::sps::Bits;

/// Width and height in pixels, after the conformance window, from an
/// HEVCDecoderConfigurationRecord (`hvcC`).
pub fn size_from_hvcc(record: &[u8]) -> Option<(u32, u32)> {
    let arrays = *record.get(22)?;
    let mut at = 23;
    for _ in 0..arrays {
        let nal_type = *record.get(at)? & 0x3f;
        let count = u16::from_be_bytes([*record.get(at + 1)?, *record.get(at + 2)?]);
        at += 3;
        for _ in 0..count {
            let len = usize::from(u16::from_be_bytes([*record.get(at)?, *record.get(at + 1)?]));
            let nal = record.get(at + 2..at + 2 + len)?;
            if nal_type == 33 {
                return sps_size(nal);
            }
            at += 2 + len;
        }
    }
    None
}

/// Width and height from one SPS NAL unit, its two byte header included.
pub fn sps_size(nal: &[u8]) -> Option<(u32, u32)> {
    let mut b = Bits::new(nal.get(2..)?);
    b.bits(4)?; // sps_video_parameter_set_id
    let sub_layers = b.bits(3)?; // sps_max_sub_layers_minus1
    b.bits(1)?; // sps_temporal_id_nesting_flag
    skip_profile_tier_level(&mut b, sub_layers)?;
    b.ue()?; // sps_seq_parameter_set_id
    let chroma = b.ue()?;
    if chroma == 3 {
        b.bits(1)?; // separate_colour_plane_flag
    }
    let (mut width, mut height) = (b.ue()?, b.ue()?);
    if b.bit()? == 1 {
        let (sub_w, sub_h) = match chroma {
            1 => (2, 2),
            2 => (2, 1),
            _ => (1, 1),
        };
        let (left, right, top, bottom) = (b.ue()?, b.ue()?, b.ue()?, b.ue()?);
        width = width.checked_sub(sub_w * (left + right))?;
        height = height.checked_sub(sub_h * (top + bottom))?;
    }
    Some((width, height))
}

/// profile_tier_level(1, max_sub_layers_minus1): 88 bits of general profile,
/// 8 of level, then per sub layer what its flags say is present.
fn skip_profile_tier_level(b: &mut Bits, sub_layers: u32) -> Option<()> {
    for _ in 0..3 {
        b.bits(32)?;
    }
    let mut present = Vec::new();
    for _ in 0..sub_layers {
        present.push((b.bit()?, b.bit()?));
    }
    if sub_layers > 0 {
        for _ in sub_layers..8 {
            b.bits(2)?;
        }
    }
    for (profile, level) in present {
        if profile == 1 {
            b.bits(32)?;
            b.bits(32)?;
            b.bits(24)?;
        }
        if level == 1 {
            b.bits(8)?;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gstreamer as gst;
    use gstreamer::prelude::*;

    /// The hvcC x265 and h265parse make for a picture of this size.
    fn hvcc(width: u32, height: u32) -> Vec<u8> {
        gmx_netkit::init().unwrap();
        let line = format!(
            "videotestsrc num-buffers=1 ! video/x-raw,format=I420,width={width},height={height} ! x265enc \
             ! h265parse ! video/x-h265,stream-format=hvc1,alignment=au ! appsink name=out"
        );
        let p = gst::parse::launch(&line).unwrap();
        let sink = p.downcast_ref::<gst::Bin>().unwrap().by_name("out").unwrap().downcast::<gstreamer_app::AppSink>().unwrap();
        p.set_state(gst::State::Playing).unwrap();
        let sample = sink.try_pull_sample(gst::ClockTime::from_seconds(10)).unwrap_or_else(|| {
            let said = p.bus().and_then(|b| b.pop_filtered(&[gst::MessageType::Error])).map(|m| format!("{m:?}"));
            panic!("x265 made no frame in ten seconds; the bus said {said:?}")
        });
        let data = sample.caps().unwrap().structure(0).unwrap().get::<gst::Buffer>("codec_data").unwrap();
        let _ = p.set_state(gst::State::Null);
        let v = data.map_readable().unwrap().to_vec();
        v
    }

    #[test]
    fn the_size_comes_out_of_what_x265_writes_cropped_or_not() {
        gmx_netkit::init().unwrap();
        if gst::ElementFactory::find("x265enc").is_none() {
            eprintln!("skipping: needs x265enc");
            return;
        }
        assert_eq!(size_from_hvcc(&hvcc(1280, 720)), Some((1280, 720)));
        // Not a multiple of the coding block: the encoder pads and crops back.
        assert_eq!(size_from_hvcc(&hvcc(426, 238)), Some((426, 238)));
        assert_eq!(size_from_hvcc(&[0u8; 10]), None);
    }
}

