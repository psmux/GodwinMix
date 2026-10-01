//! Which chain decodes what, and reading numbers out of what comes back.

use gstreamer as gst;

use super::chain::Chain;
use super::picture::Luma;

/// How wide a thumbnail is. The height follows the picture's shape.
pub const THUMB_WIDTH: i32 = 320;

/// The decoders tried for each video codec, best first: the machine's
/// hardware decoder behind a parser (which puts the size in the caps a
/// hardware decoder wants), then libav or dav1d on the CPU.
fn decoders(kind: &str) -> &'static [&'static [&'static str]] {
    match kind {
        "video/x-h264" => &[&["h264parse", "vtdec_hw"], &["h264parse", "vah264dec"], &["h264parse", "nvh264dec"], &["h264parse", "d3d11h264dec"], &["avdec_h264"]],
        "video/x-h265" => &[&["h265parse", "vtdec_hw"], &["h265parse", "vah265dec"], &["h265parse", "nvh265dec"], &["h265parse", "d3d11h265dec"], &["avdec_h265"]],
        "video/x-av1" => &[&["av1parse", "vtdec_hw"], &["av1parse", "vaav1dec"], &["dav1ddec"], &["avdec_av1"]],
        _ => &[],
    }
}

fn exists(names: &[&str]) -> bool {
    names.iter().all(|n| gst::ElementFactory::find(n).is_some())
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
/// without pictures and says nothing more. `cpu` skips the hardware
/// decoders, for a worker whose hardware decoder has refused.
pub fn chain_for(kind: &str, cpu: bool) -> Option<Chain> {
    let scale = ["videoscale", "videoconvert"];
    if let Some(names) = decoders(kind).iter().filter(|d| !cpu || d.len() == 1).find(|d| exists(d)) {
        let all: Vec<&str> = names.iter().chain(scale.iter()).copied().collect();
        return Chain::new(&all, thumb_caps()).ok();
    }
    let built = match kind {
        "video/x-raw" => Chain::new(&scale, thumb_caps()),
        "audio/mpeg" => {
            let out = gst::Caps::builder("audio/x-raw").field("format", "F32LE").field("layout", "interleaved").build();
            let dec = ["avdec_aac", "fdkaacdec"].into_iter().find(|d| exists(&[d]))?;
            Chain::new(&[dec, "audioconvert"], out)
        }
        jpeg if jpeg.starts_with("jpeg/") => {
            // A thumbnail of the asked size, from the 320 pixel picture.
            let (w, h) = jpeg[5..].split_once('x')?;
            let (width, height): (i32, i32) = (w.parse().ok()?, h.parse().ok()?);
            let out = gst::Caps::builder("image/jpeg").field("width", width).field("height", height).build();
            Chain::new(&["videoscale", "jpegenc"], out)
        }
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
