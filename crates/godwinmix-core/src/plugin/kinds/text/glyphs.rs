//! Letters to pixels, once per change, with GStreamer's own `textoverlay`.
//!
//! `textoverlay` is Pango and Cairo, which the official GStreamer packages
//! ship on Windows, macOS and Linux alike, so every mixer already has it and
//! no crate is added. It is asked for the words as an overlay rectangle
//! rather than blended into a frame: given input caps that carry the overlay
//! composition meta it attaches its rendering as a straight alpha picture and
//! leaves the frame alone. That picture, outline and shadow included, is what
//! comes back from here.
//!
//! The pipeline is built, run for one buffer and taken down each time. That
//! is a few milliseconds on a worker thread, once per change of the words or
//! the look, and nothing at all between changes.

use super::style::{Align, Look};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

/// Straight alpha RGBA, `width * 4` to a row.
pub struct Glyphs {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// The largest frame the words are laid out in. Cairo refuses surfaces over
/// 32767 pixels on a side; a ticker strip longer than this is cut.
pub const MAX_SIDE: u32 = 32000;

/// What to render: the words, the look at `scale`, and the width to wrap at
/// (`None` lays every line out at its own length).
pub struct Ask<'a> {
    pub text: &'a str,
    pub look: &'a Look,
    pub scale: f64,
    pub wrap: Option<u32>,
}

/// Render the words, or `None` for no words at all.
pub fn render(ask: &Ask<'_>) -> Result<Option<Glyphs>> {
    if ask.text.trim().is_empty() {
        return Ok(None);
    }
    let px = (ask.look.size * ask.scale).max(1.0);
    let lines = ask.text.lines().count().max(1) as f64;
    let longest = ask.text.lines().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
    let width = ask.wrap.unwrap_or((longest * px * 1.2 + px * 2.0) as u32).clamp(16, MAX_SIDE);
    let wrapped_lines = if ask.wrap.is_some() { lines * (longest * px * 0.7 / width as f64).ceil().max(1.0) } else { lines };
    let height = ((wrapped_lines * px * 1.6) + px * 2.0).clamp(16.0, MAX_SIDE as f64) as u32;
    let rect = run(ask, width & !1, (height + 1) & !1)?;
    Ok(rect)
}

fn run(ask: &Ask<'_>, width: u32, height: u32) -> Result<Option<Glyphs>> {
    let caps = gst_video::VideoInfo::builder(gst_video::VideoFormat::Bgra, width, height)
        .fps(gst::Fraction::new(1, 1))
        .build()
        .context("describing the text frame")?
        .to_caps()
        .context("text frame caps")?;
    let mut caps = caps;
    caps.make_mut().set_features_simple(Some(gst::CapsFeatures::new([
        gst_video::CAPS_FEATURE_META_GST_VIDEO_OVERLAY_COMPOSITION,
    ])));
    let src = gst_app::AppSrc::builder().caps(&caps).format(gst::Format::Time).build();
    let overlay = crate::gstutil::make("textoverlay", "text-render")?;
    configure(&overlay, ask);
    let sink = gst_app::AppSink::builder().caps(&caps).sync(false).build();
    let pipeline = gst::Pipeline::new();
    pipeline.add_many([src.upcast_ref(), &overlay, sink.upcast_ref()]).context("adding the text renderer")?;
    gst::Element::link_many([src.upcast_ref(), &overlay, sink.upcast_ref()]).context("linking the text renderer")?;
    pipeline.set_state(gst::State::Playing).context("starting the text renderer")?;
    let mut frame = gst::Buffer::with_size((width * height * 4) as usize).context("allocating the text frame")?;
    frame.get_mut().context("new buffer")?.set_pts(gst::ClockTime::ZERO);
    let _ = src.push_buffer(frame);
    let _ = src.end_of_stream();
    let sample = sink.try_pull_sample(gst::ClockTime::from_seconds(5));
    let _ = pipeline.set_state(gst::State::Null);
    let sample = sample.context("textoverlay rendered nothing in five seconds")?;
    let buffer = sample.buffer().context("textoverlay sent a sample with no buffer")?;
    picture(buffer)
}

fn configure(overlay: &gst::Element, ask: &Ask<'_>) {
    let look = ask.look;
    overlay.set_property("text", ask.text);
    overlay.set_property("font-desc", look.font_desc(ask.scale));
    crate::probe::set_enum(overlay, "valignment", "top");
    crate::probe::set_enum(overlay, "halignment", "left");
    let line = match look.align {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
    };
    crate::probe::set_enum(overlay, "line-alignment", line);
    crate::probe::set_enum(overlay, "wrap-mode", if ask.wrap.is_some() { "wordchar" } else { "none" });
    crate::probe::set_int(overlay, "xpad", 0);
    crate::probe::set_int(overlay, "ypad", 0);
    overlay.set_property("color", argb(&look.color).unwrap_or(0xffff_ffff));
    let outline = argb(&look.outline);
    overlay.set_property("draw-outline", outline.is_some());
    if let Some(o) = outline {
        overlay.set_property("outline-color", o);
    }
    overlay.set_property("draw-shadow", look.shadow);
    overlay.set_property("shaded-background", false);
    // On by default, and it scales the letters by the frame width over 640,
    // which here is only as wide as the words. Sizes are pixels, said once.
    overlay.set_property("auto-resize", false);
}

fn argb(s: &str) -> Option<u32> {
    if s.trim().is_empty() {
        return None;
    }
    let [r, g, b, a] = super::style::colour(s).ok()?;
    Some(u32::from_be_bytes([a, r, g, b]))
}

/// The one rectangle `textoverlay` attached, as straight RGBA.
fn picture(buffer: &gst::BufferRef) -> Result<Option<Glyphs>> {
    let Some(meta) = buffer.meta::<gst_video::VideoOverlayCompositionMeta>() else { return Ok(None) };
    let composition = meta.overlay();
    let Ok(rect) = composition.rectangle(0) else { return Ok(None) };
    let pixels = rect.pixels_unscaled_argb(gst_video::VideoOverlayFormatFlags::empty());
    let vmeta = pixels.meta::<gst_video::VideoMeta>().context("the text rendering has no layout")?;
    let (w, h, stride) = (vmeta.width(), vmeta.height(), vmeta.stride()[0] as usize);
    let map = pixels.map_readable().context("reading the text rendering")?;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            // BGRA in memory on a little endian machine, ARGB on a big one:
            // the overlay format is "ARGB as a 32 bit word".
            let p = y * stride + x * 4;
            let word = u32::from_ne_bytes([map[p], map[p + 1], map[p + 2], map[p + 3]]);
            let [a, r, g, b] = word.to_be_bytes();
            rgba[(y * w as usize + x) * 4..][..4].copy_from_slice(&[r, g, b, a]);
        }
    }
    Ok(Some(Glyphs { rgba, width: w, height: h }))
}
