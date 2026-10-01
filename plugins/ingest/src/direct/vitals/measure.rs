//! Which chain decodes what, and reading numbers out of what comes back.

use gstreamer as gst;

use super::chain::Chain;
use super::picture::Luma;

/// How wide a thumbnail is. The height follows the picture's shape.
pub const THUMB_WIDTH: i32 = 320;

/// The first decoder of these this machine has.
fn first_of(names: &[&'static str]) -> Option<&'static str> {
    names.iter().copied().find(|n| gst::ElementFactory::find(n).is_some())
}

fn thumb_caps() -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", "I420")
        .field("width", THUMB_WIDTH)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build()
}

/// The chain for one kind of input: a keyframe of each video codec the hub
/// carries, a frame already decoded, AAC, or a thumbnail to make a JPEG of.
/// `None` when this machine has no decoder for it, which leaves that show
/// without pictures and says nothing more.
pub fn chain_for(kind: &str) -> Option<Chain> {
    let built = match kind {
        "video/x-h264" => Chain::new(first_of(&["avdec_h264"])?, &["videoscale", "videoconvert"], thumb_caps()),
        "video/x-h265" => Chain::new(first_of(&["avdec_h265"])?, &["videoscale", "videoconvert"], thumb_caps()),
        "video/x-av1" => Chain::new(first_of(&["dav1ddec", "avdec_av1"])?, &["videoscale", "videoconvert"], thumb_caps()),
        "video/x-raw" => Chain::new("videoscale", &["videoconvert"], thumb_caps()),
        "audio/mpeg" => {
            let out = gst::Caps::builder("audio/x-raw").field("format", "F32LE").field("layout", "interleaved").build();
            Chain::new(first_of(&["avdec_aac", "fdkaacdec"])?, &["audioconvert"], out)
        }
        "jpeg" => Chain::new("jpegenc", &[], gst::Caps::new_empty_simple("image/jpeg")),
        _ => return None,
    };
    built.ok()
}

pub fn size(caps: &gst::CapsRef) -> (u32, u32) {
    let Some(s) = caps.structure(0) else { return (0, 0) };
    let get = |k: &str| s.get::<i32>(k).unwrap_or(0).max(0) as u32;
    (get("width"), get("height"))
}

/// The luma of a thumbnail sized I420 sample, sub sampled. The plane is at
/// the front of the buffer with GStreamer's default stride, rows rounded up
/// to four bytes.
pub fn luma(sample: &gst::Sample) -> Option<Luma> {
    let (w, h) = size(sample.caps()?);
    let map = sample.buffer()?.map_readable().ok()?;
    let stride = (w as usize).div_ceil(4) * 4;
    Luma::sample(map.as_slice(), w as usize, h as usize, stride)
}

/// The largest absolute value in a sample of interleaved 32 bit floats.
pub fn peak(sample: &gst::Sample) -> Option<f64> {
    let map = sample.buffer()?.map_readable().ok()?;
    let floats = map.as_slice().chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs());
    floats.reduce(f32::max).map(f64::from)
}

/// A linear peak in dBFS. Digital silence is minus infinity, written as -120.
pub fn to_db(peak: f64) -> f64 {
    if peak <= 1e-6 {
        -120.0
    } else {
        20.0 * peak.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_scale_is_zero_and_nothing_is_the_floor() {
        assert_eq!(to_db(1.0), 0.0);
        assert!((to_db(0.5) + 6.02).abs() < 0.01);
        assert_eq!(to_db(0.0), -120.0);
    }
}
