//! A transparent picture to AYUV, once: decode a raster file at its own size,
//! render an SVG at the size it is wanted, and scale a held picture.
//!
//! Each is a pipeline run for one buffer and taken down, on the render worker
//! thread, never on the mixer thread or a streaming thread.

use crate::gstutil::make;
use crate::overlay::Picture;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

/// AYUV caps at BT.709 limited range, the canvas's own, at a size or not.
fn ayuv(size: Option<(u32, u32)>) -> gst::Caps {
    let mut b = gst::Caps::builder("video/x-raw").field("format", "AYUV").field("colorimetry", crate::caps::COLORIMETRY);
    if let Some((w, h)) = size {
        b = b.field("width", w as i32).field("height", h as i32);
    }
    b.build()
}

/// Decode the first picture at `uri` with whatever decoder GStreamer picks.
pub fn raster(uri: &str) -> Result<Picture> {
    let src = gst::Element::make_from_uri(gst::URIType::Src, uri, None).with_context(|| format!("nothing in this build reads {uri}"))?;
    let decode = make("decodebin", "still-decode")?;
    let convert = make("videoconvert", "still-convert")?;
    let tail = [convert.clone()];
    run(&[src.clone(), decode.clone()], &tail, ayuv(None), Some((&decode, &convert)))
}

/// Render the SVG at `uri` at `size`: `rsvgdec` draws it at that size, so a
/// logo placed small is drawn small and one placed large is drawn sharp.
pub fn svg(uri: &str, size: (u32, u32)) -> Result<Picture> {
    if !crate::probe::exists("rsvgdec") {
        return Err(crate::setup::system::missing_error("Drawing SVG pictures", crate::setup::system::BAD, &["rsvgdec"]));
    }
    let src = gst::Element::make_from_uri(gst::URIType::Src, uri, None).with_context(|| format!("nothing in this build reads {uri}"))?;
    // A file source says nothing about what it reads, and `rsvgdec` will not
    // take bytes with no caps on them.
    let typed = make("typefind", "svg-typed")?;
    let dec = make("rsvgdec", "svg-decode")?;
    let convert = make("videoconvert", "svg-convert")?;
    run(&[src, typed, dec], &[convert], ayuv(Some(size)), None)
}

/// Render SVG held in memory at `size`: a template filled with its fields.
/// The same `rsvgdec` as a file, given the bytes through an `appsrc`.
pub fn svg_data(svg: &str, size: (u32, u32)) -> Result<Picture> {
    anyhow::ensure!(crate::probe::exists("rsvgdec"), "this build of GStreamer has no rsvgdec (gst-plugins-bad, built with librsvg), so it cannot draw an SVG template. Install it");
    let caps = gst::Caps::builder("image/svg+xml").build();
    let src = gst_app::AppSrc::builder().caps(&caps).format(gst::Format::Time).build();
    let mut buffer = gst::Buffer::from_slice(svg.as_bytes().to_vec());
    buffer.get_mut().context("new buffer")?.set_pts(gst::ClockTime::ZERO);
    let _ = src.push_buffer(buffer);
    let _ = src.end_of_stream();
    let dec = make("rsvgdec", "svg-decode")?;
    let convert = make("videoconvert", "svg-convert")?;
    run(&[src.upcast(), dec], &[convert], ayuv(Some(size)), None)
}

/// Build `head`, then `tail`, then a capsfilter and an appsink, link them
/// (the decodebin pair by its pad when there is one), run until one frame
/// arrives.
fn run(head: &[gst::Element], tail: &[gst::Element], caps: gst::Caps, dynamic: Option<(&gst::Element, &gst::Element)>) -> Result<Picture> {
    let filter = crate::gstutil::capsfilter("still-caps", &caps)?;
    let sink = gst_app::AppSink::builder().sync(false).max_buffers(1).build();
    let pipeline = gst::Pipeline::new();
    let mut all: Vec<&gst::Element> = head.iter().chain(tail).collect();
    all.push(&filter);
    all.push(sink.upcast_ref());
    pipeline.add_many(&all).context("adding the picture decoder")?;
    gst::Element::link_many(head).context("linking the picture reader")?;
    let after: Vec<&gst::Element> = tail.iter().chain([&filter, sink.upcast_ref()]).collect();
    gst::Element::link_many(&after).context("linking the picture converter")?;
    match dynamic {
        Some((decode, convert)) => {
            let convert = convert.clone();
            decode.connect_pad_added(move |_, pad| {
                if let Some(sink) = convert.static_pad("sink").filter(|s| !s.is_linked()) {
                    let _ = pad.link(&sink);
                }
            });
        }
        None => {
            let (last, first) = (head.last().context("nothing to read with")?, after[0]);
            last.link(first).context("linking the picture to its converter")?;
        }
    }
    pipeline.set_state(gst::State::Playing).context("starting the picture decoder")?;
    let sample = sink.try_pull_preroll(gst::ClockTime::from_seconds(10));
    let _ = pipeline.set_state(gst::State::Null);
    picture(&sample.context("the picture did not decode in ten seconds; is it a picture?")?)
}

fn picture(sample: &gst::Sample) -> Result<Picture> {
    let caps = sample.caps().context("a decoded picture with no caps")?;
    let info = gst_video::VideoInfo::from_caps(caps).context("a decoded picture that is not video")?;
    let buffer = sample.buffer_owned().context("a decoded picture with no buffer")?;
    let (w, h) = (info.width(), info.height());
    Ok(Picture { buffer, width: w, height: h, stride: info.stride()[0] as usize, natural: (w, h), content: None, within: None, keyed: None })
}

/// `pic` at `size`, scaled once. Keeps the natural size it had.
pub fn scaled(pic: &Picture, size: (u32, u32)) -> Result<Picture> {
    let info = |w: u32, h: u32| {
        gst_video::VideoInfo::builder(gst_video::VideoFormat::Ayuv, w, h).fps(gst::Fraction::new(0, 1)).build()
    };
    let from = info(pic.width, pic.height).context("the held picture's layout")?;
    let to = info(size.0.max(1), size.1.max(1)).context("the wanted size")?;
    let mut src = gst::Buffer::with_size(from.size()).context("allocating")?;
    {
        let map = pic.buffer.map_readable().context("reading the held picture")?;
        let dst = src.get_mut().context("new buffer")?;
        let mut w = dst.map_writable().context("writing")?;
        for row in 0..pic.height as usize {
            let (a, b) = (row * pic.stride, row * from.stride()[0] as usize);
            w[b..b + pic.width as usize * 4].copy_from_slice(&map[a..a + pic.width as usize * 4]);
        }
    }
    let mut out = gst::Buffer::with_size(to.size()).context("allocating the scaled picture")?;
    let conv = gst_video::VideoConverter::new(&from, &to, None).context("a scaler for the picture")?;
    let in_frame = gst_video::VideoFrameRef::from_buffer_ref_readable(src.as_ref(), &from).map_err(|_| anyhow::anyhow!("mapping the picture"))?;
    let mut out_frame = gst_video::VideoFrameRef::from_buffer_ref_writable(out.get_mut().context("new buffer")?, &to)
        .map_err(|_| anyhow::anyhow!("mapping the scaled picture"))?;
    conv.frame_ref(&in_frame, &mut out_frame);
    drop(out_frame);
    Ok(Picture { buffer: out, width: to.width(), height: to.height(), stride: to.stride()[0] as usize, natural: pic.natural, content: None, within: None, keyed: None })
}
