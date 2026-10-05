//! Pixels for a preview: what shows through the transparent parts, a held
//! picture turned to RGBA at the size it is drawn, and one laid over the
//! other.

use crate::overlay::Picture;
use anyhow::{bail, Context, Result};
use gstreamer as gst;
use gstreamer_video as gst_video;
use image::RgbImage;

/// What shows through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    /// Grey squares, the convention for "nothing is here".
    Checker,
    Colour([u8; 3]),
}

impl Backdrop {
    /// `checker` (or nothing), `black`, `white`, `grey`, or `#rrggbb`.
    pub fn parse(text: Option<&str>) -> Result<Backdrop> {
        let t = text.map(|t| t.trim().to_ascii_lowercase()).unwrap_or_default();
        Ok(match t.as_str() {
            "" | "checker" | "checkerboard" | "transparent" => Backdrop::Checker,
            "black" => Backdrop::Colour([0, 0, 0]),
            "white" => Backdrop::Colour([255, 255, 255]),
            "grey" | "gray" => Backdrop::Colour([128, 128, 128]),
            hex => match crate::plugin::filters::chroma::parse_hex(hex) {
                Some(rgb) => Backdrop::Colour(rgb),
                None => bail!("background is {hex:?}; give checker, black, white, grey or a colour such as #1a2b3c"),
            },
        })
    }

    /// A name for the cache file, with no underscore in it.
    pub fn key(self) -> String {
        match self {
            Backdrop::Checker => "checker".into(),
            Backdrop::Colour([r, g, b]) => format!("{r:02x}{g:02x}{b:02x}"),
        }
    }

    pub fn fill(self, w: u32, h: u32) -> RgbImage {
        let square = (w / 32).max(8);
        RgbImage::from_fn(w, h, |x, y| match self {
            Backdrop::Colour(c) => image::Rgb(c),
            Backdrop::Checker if ((x / square) + (y / square)) % 2 == 0 => image::Rgb([70, 70, 74]),
            Backdrop::Checker => image::Rgb([96, 96, 100]),
        })
    }
}

/// RGBA pixels and where they go.
pub struct Layer {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}

/// `pic` as RGBA at `size`, scaled once by GStreamer's own converter.
pub fn from_picture(pic: &Picture, size: (u32, u32), at: (i32, i32)) -> Result<Layer> {
    let (w, h) = (size.0.max(1), size.1.max(1));
    let from = gst_video::VideoInfo::builder(gst_video::VideoFormat::Ayuv, pic.width, pic.height)
        .colorimetry(&crate::caps::COLORIMETRY.parse().context("the canvas colorimetry")?)
        .build()
        .context("the picture's layout")?;
    let to = gst_video::VideoInfo::builder(gst_video::VideoFormat::Rgba, w, h).build().context("the preview's layout")?;
    let mut src = gst::Buffer::with_size(from.size()).context("allocating")?;
    {
        let map = pic.buffer.map_readable().context("reading the picture")?;
        let dst = src.get_mut().context("new buffer")?;
        let mut out = dst.map_writable().context("writing")?;
        let row = pic.width as usize * 4;
        for y in 0..pic.height as usize {
            let (a, b) = (y * pic.stride, y * from.stride()[0] as usize);
            out[b..b + row].copy_from_slice(&map[a..a + row]);
        }
    }
    let mut out = gst::Buffer::with_size(to.size()).context("allocating the preview")?;
    let conv = gst_video::VideoConverter::new(&from, &to, None).context("a converter for the preview")?;
    let in_frame = gst_video::VideoFrameRef::from_buffer_ref_readable(src.as_ref(), &from).map_err(|_| anyhow::anyhow!("mapping the picture"))?;
    let mut out_frame = gst_video::VideoFrameRef::from_buffer_ref_writable(out.get_mut().context("new buffer")?, &to)
        .map_err(|_| anyhow::anyhow!("mapping the preview"))?;
    conv.frame_ref(&in_frame, &mut out_frame);
    let stride = to.stride()[0] as usize;
    drop(out_frame);
    let map = out.map_readable().context("reading the preview")?;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        rgba.extend_from_slice(&map[y * stride..y * stride + w as usize * 4]);
    }
    Ok(Layer { rgba, width: w, height: h, x: at.0, y: at.1 })
}

/// Lay `layer` over `img`, by its alpha, clipped to the picture.
pub fn blend(img: &mut RgbImage, layer: &Layer) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    for ly in 0..layer.height as i32 {
        let y = layer.y + ly;
        if y < 0 || y >= ih {
            continue;
        }
        for lx in 0..layer.width as i32 {
            let x = layer.x + lx;
            if x < 0 || x >= iw {
                continue;
            }
            let i = ((ly as u32 * layer.width + lx as u32) * 4) as usize;
            let a = layer.rgba[i + 3] as u32;
            if a == 0 {
                continue;
            }
            let px = img.get_pixel_mut(x as u32, y as u32);
            for c in 0..3 {
                px.0[c] = ((layer.rgba[i + c] as u32 * a + px.0[c] as u32 * (255 - a)) / 255) as u8;
            }
        }
    }
}
