//! A cube drawn as a picture.
//!
//! Two layouts are read, told apart by the picture's size:
//!
//! * 512 x 512 is OBS's: 64 points a side, eight tiles across and eight down,
//!   blue choosing the tile (left to right, then down), red across a tile
//!   and green down it.
//! * Any other square whose side is a cube, `L^3` for a level `L` (64 x 64,
//!   729 x 729, 1728 x 1728, 4096 x 4096), is a Hald CLUT: `L^2` points a side,
//!   read in raster order with red fastest, then green, then blue.
//!
//! The picture is decoded once, at import, by GStreamer's own decoder, so no
//! picture library is linked for it. The library keeps what it read as a
//! `.cube` and a frame never sees the picture.

use super::{Cube, Lut, MAX_SIZE};
use anyhow::{bail, Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

pub fn parse(bytes: &[u8]) -> Result<Lut> {
    let (w, h, rgb) = decode(bytes)?;
    from_rgb(w, h, &rgb)
}

/// The cube in an RGB picture `w` by `h`, three bytes a pixel, rows packed.
pub fn from_rgb(w: usize, h: usize, rgb: &[u8]) -> Result<Lut> {
    if w != h {
        bail!("a LUT picture is square, and this one is {w} x {h}");
    }
    let px = |x: usize, y: usize| {
        let i = (y * w + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]].map(|c| c as f32 / 255.0)
    };
    let (size, at): (usize, Box<dyn Fn(usize, usize, usize) -> [f32; 3]>) = if w == 512 {
        (64, Box::new(|r, g, b| px((b % 8) * 64 + r, (b / 8) * 64 + g)))
    } else {
        let level = (1..=16).find(|l| l * l * l == w).with_context(|| {
            format!("a {w} x {w} picture is neither OBS's 512 x 512 nor a Hald CLUT, whose side is a cube: 64, 512, 729, 1728 or 4096")
        })?;
        let n = level * level;
        (n, Box::new(move |r, g, b| {
            let i = r + n * (g + n * b);
            px(i % w, i / w)
        }))
    };
    if size > MAX_SIZE {
        // A level 16 Hald is 256 a side; keep every fourth point, which is
        // still finer than any grade needs.
        let step = (size - 1).div_ceil(MAX_SIZE - 1);
        let kept = (size - 1) / step + 1;
        return Ok(Lut::from_fn("", kept, |c| {
            let i = c.map(|v| ((v * (kept - 1) as f32).round() as usize * step).min(size - 1));
            at(i[0], i[1], i[2])
        }));
    }
    let mut values = Vec::with_capacity(size * size * size);
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                values.push(at(r, g, b));
            }
        }
    }
    Ok(Lut { title: String::new(), shaper: None, cube: Some(Cube { size, min: [0.0; 3], max: [1.0; 3], values }) })
}

/// The picture as packed RGB, by appsrc, decodebin and an appsink.
fn decode(bytes: &[u8]) -> Result<(usize, usize, Vec<u8>)> {
    gst::init().context("starting GStreamer to read the picture")?;
    let pipeline = gst::parse::launch(
        "appsrc name=in ! decodebin ! videoconvert ! video/x-raw,format=RGB ! appsink name=out sync=false",
    )
    .context("making the picture decoder")?
    .downcast::<gst::Pipeline>()
    .map_err(|_| anyhow::anyhow!("the picture decoder is not a pipeline"))?;
    let src = pipeline.by_name("in").and_then(|e| e.downcast::<gst_app::AppSrc>().ok()).context("no appsrc")?;
    let sink = pipeline.by_name("out").and_then(|e| e.downcast::<gst_app::AppSink>().ok()).context("no appsink")?;
    pipeline.set_state(gst::State::Playing).context("starting the picture decoder")?;
    let _ = src.push_buffer(gst::Buffer::from_slice(bytes.to_vec()));
    let _ = src.end_of_stream();
    let sample = sink.try_pull_sample(gst::ClockTime::from_seconds(10));
    let _ = pipeline.set_state(gst::State::Null);
    let sample = sample.context("the picture would not decode. Save it as a PNG, 8 bits a channel, with no colour profile")?;
    let caps = sample.caps().context("the decoded picture has no caps")?;
    let info = gstreamer_video::VideoInfo::from_caps(caps).context("reading the picture's size")?;
    let buffer = sample.buffer().context("the decoded picture has no buffer")?;
    let map = buffer.map_readable().context("reading the decoded picture")?;
    let (w, h, stride) = (info.width() as usize, info.height() as usize, info.stride()[0] as usize);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for row in 0..h {
        rgb.extend_from_slice(&map[row * stride..row * stride + w * 3]);
    }
    Ok((w, h, rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An identity picture in OBS's layout.
    fn obs_identity() -> Vec<u8> {
        let mut rgb = vec![0u8; 512 * 512 * 3];
        for b in 0..64 {
            for g in 0..64 {
                for r in 0..64 {
                    let (x, y) = ((b % 8) * 64 + r, (b / 8) * 64 + g);
                    let i = (y * 512 + x) * 3;
                    rgb[i..i + 3].copy_from_slice(&[r, g, b].map(|v| (v * 255 / 63) as u8));
                }
            }
        }
        rgb
    }

    #[test]
    fn obs_square_reads_as_an_identity() {
        let lut = from_rgb(512, 512, &obs_identity()).unwrap();
        assert_eq!(lut.size(), 64);
        let out = lut.sample([0.25, 0.5, 0.75]);
        for (o, want) in out.iter().zip([0.25, 0.5, 0.75]) {
            assert!((o - want).abs() < 0.01, "{out:?}");
        }
    }

    #[test]
    fn a_level_four_hald_reads() {
        // Level 4: 64 x 64 pixels, 16 points a side.
        let n = 16;
        let mut rgb = vec![0u8; 64 * 64 * 3];
        for i in 0..n * n * n {
            let (r, g, b) = (i % n, (i / n) % n, i / (n * n));
            rgb[i * 3..i * 3 + 3].copy_from_slice(&[r, g, b].map(|v| (v * 17) as u8));
        }
        let lut = from_rgb(64, 64, &rgb).unwrap();
        assert_eq!(lut.size(), 16);
        let out = lut.sample([1.0, 0.0, 0.0]);
        assert!((out[0] - 1.0).abs() < 1e-6 && out[1] < 1e-6, "{out:?}");
    }

    #[test]
    fn an_odd_size_says_which_sizes_are_read() {
        let err = from_rgb(100, 100, &vec![0; 100 * 100 * 3]).unwrap_err().to_string();
        assert!(err.contains("512 x 512"), "{err}");
    }
}
