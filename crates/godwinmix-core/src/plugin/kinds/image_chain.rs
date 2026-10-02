//! What an address means to `image/source`, and the elements that turn it
//! into a picture.

use super::super::BuildCtx;
use crate::config::Params;
use crate::gstutil::make;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

const EXTENSIONS: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("bmp", "image/bmp"),
    ("webp", "image/webp"),
    ("tif", "image/tiff"),
    ("tiff", "image/tiff"),
    ("svg", "image/svg+xml"),
];

/// The image type an address names by its extension, if it names one.
pub fn image_type(uri: &str) -> Option<&'static str> {
    let lower = uri.trim().to_lowercase();
    let path = lower.split(['?', '#']).next().unwrap_or(&lower);
    let ext = path.rsplit_once('.')?.1;
    EXTENSIONS.iter().find(|(e, _)| *e == ext).map(|(_, t)| *t)
}

/// Whether the address is a numbered pattern: `%d` or `%04d` in the name.
pub fn is_sequence(uri: &str) -> bool {
    let name = uri.rsplit(['/', '\\']).next().unwrap_or(uri);
    name.split('%').skip(1).any(|rest| rest.trim_start_matches(|c: char| c.is_ascii_digit()).starts_with('d'))
}

/// `params.fps` for a sequence: 1 to 120, 25 when left out.
pub fn fps(params: &Params) -> Result<i32> {
    match params.get("fps") {
        None => Ok(25),
        Some(v) => v
            .as_integer()
            .filter(|n| (1..=120).contains(n))
            .map(|n| n as i32)
            .context("image/source params.fps must be a whole number of pictures a second, 1 to 120"),
    }
}

/// The elements of one picture chain: `before` link into `dynamic`, whose
/// pads appear later and go to the first of `after`; the last of `after`
/// goes to the canvas.
pub struct Picture {
    pub before: Vec<gst::Element>,
    pub dynamic: gst::Element,
    pub after: Vec<gst::Element>,
}

/// uridecodebin, then imagefreeze repeating the one frame live.
pub fn still(ctx: &BuildCtx, uri: &str) -> Result<Picture> {
    let decode = make("uridecodebin", &format!("{}-src-image", ctx.id))?;
    decode.set_property("uri", crate::input::to_uri(uri));
    let convert = make("videoconvert", &format!("{}-image-convert", ctx.id))?;
    let freeze = make("imagefreeze", &format!("{}-image-freeze", ctx.id))?;
    freeze.set_property("is-live", true);
    Ok(Picture { before: Vec::new(), dynamic: decode, after: vec![convert, freeze] })
}

/// multifilesrc looping at `fps`, then decodebin.
pub fn sequence(ctx: &BuildCtx, uri: &str) -> Result<Picture> {
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let kind = image_type(path).context("a numbered sequence needs a picture extension, such as frames/%04d.png")?;
    let first = first_index(path).with_context(|| {
        format!("there is no picture numbered 0 to 9999 at {path} yet. Upload the pictures in the Media tab, then try again.")
    })?;
    let src = make("multifilesrc", &format!("{}-src-sequence", ctx.id))?;
    src.set_property("location", path);
    src.set_property("loop", true);
    crate::probe::set_int(&src, "index", first);
    crate::probe::set_int(&src, "start-index", first);
    let rate = fps(&ctx.cfg.effective_params())?;
    src.set_property("caps", gst::Caps::builder(kind).field("framerate", gst::Fraction::new(rate, 1)).build());
    let decode = make("decodebin", &format!("{}-sequence-decode", ctx.id))?;
    let convert = make("videoconvert", &format!("{}-sequence-convert", ctx.id))?;
    Ok(Picture { before: vec![src], dynamic: decode, after: vec![convert] })
}

/// The first number the pattern has a file for, looking at 0 to 9999.
fn first_index(pattern: &str) -> Option<i64> {
    (0..10_000).find(|i| std::path::Path::new(&printf_d(pattern, *i)).exists())
}

/// The pattern with its `%d` or `%0Nd` filled in.
pub fn printf_d(pattern: &str, n: i64) -> String {
    let Some((head, rest)) = pattern.rsplit_once('%') else { return pattern.to_string() };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let tail = rest.get(digits.len() + 1..).unwrap_or("");
    let width = digits.trim_start_matches('0').parse::<usize>().unwrap_or(0);
    format!("{head}{n:0width$}{tail}")
}
