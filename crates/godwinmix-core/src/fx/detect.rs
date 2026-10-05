//! Looking at a file to say what it is: decode it small, measure every
//! frame, decide.
//!
//! One pass of the whole clip at 128x72 through GStreamer, on whatever
//! thread asked (an import runs on a blocking worker, never on the mixer's
//! thread), and the frames are kept for the preview strip. For each frame:
//! how much of it is solid by its alpha, how much is near white, how much
//! is near black, and how far its colour is from grey. From those:
//!
//! * a clip whose decoder gave alpha, and used it, is a stinger, cut in the
//!   middle of the run of frames where it is most solid;
//! * a clip that is mostly black, or starts or ends on black, is an overlay
//!   for Screen, cut in the middle of its whitest run of frames, and a transition only if that frame covers half the
//!   picture;
//! * a grey picture is a luma matte;
//! * anything else is an opaque stinger, cut half way, with a note.

use anyhow::{bail, Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use std::path::Path;
use std::sync::Arc;

/// The size frames are measured and previewed at.
pub const SMALL: (u32, u32) = (128, 72);
/// The most frames read: twenty seconds at 30 fps is longer than any stinger.
const MOST: usize = 600;

/// What one frame is like, each 0 to 1.
#[derive(Debug, Clone, Copy, Default)]
pub struct Stat {
    pub solid: f64,
    pub white: f64,
    pub black: f64,
    pub colour: f64,
}

/// A whole file, measured.
pub struct Measured {
    pub stats: Vec<Stat>,
    /// Every frame at `SMALL` in AYUV, for the preview.
    pub frames: Vec<Vec<u8>>,
    pub duration_ms: u64,
    /// The decoder's own output carried alpha.
    pub alpha: bool,
}

impl Measured {
    pub fn still(&self) -> bool {
        self.frames.len() <= 1
    }

    /// The time of frame `i`, in milliseconds from the start.
    pub fn at_ms(&self, i: usize) -> u64 {
        self.duration_ms * i as u64 / self.frames.len().max(1) as u64
    }
}

/// Decode `path` small and measure it.
pub fn measure(path: &Path) -> Result<Measured> {
    gst::init()?;
    let uri = gst::glib::filename_to_uri(path, None).context("not a file path")?;
    let desc = format!(
        "uridecodebin name=dec uri=\"{uri}\" ! videoconvert ! videoscale ! video/x-raw,format=AYUV,width={},height={},pixel-aspect-ratio=1/1 ! appsink name=out sync=false max-buffers=4",
        SMALL.0, SMALL.1
    );
    let pipeline = gst::parse::launch(&desc)?.downcast::<gst::Pipeline>().map_err(|_| anyhow::anyhow!("not a pipeline"))?;
    let alpha = Arc::new(Mutex::new(false));
    let seen = alpha.clone();
    if let Some(dec) = pipeline.by_name("dec") {
        super::player::software_only(&dec);
        dec.connect_pad_added(move |_, pad| {
            let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
            if let Ok(info) = gst_video::VideoInfo::from_caps(&caps) {
                *seen.lock() |= info.format_info().has_alpha();
            }
        });
    }
    let sink = pipeline.by_name("out").context("no sink")?.downcast::<gst_app::AppSink>().map_err(|_| anyhow::anyhow!("not an appsink"))?;
    pipeline.set_state(gst::State::Playing).context("this file would not open")?;
    let (mut frames, mut stats, mut last) = (Vec::new(), Vec::new(), 0u64);
    let mut step = 0u64;
    while frames.len() < MOST {
        let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(10)) else { break };
        let Some(buffer) = sample.buffer() else { continue };
        let pts = buffer.pts().map(|t| t.mseconds()).unwrap_or(0);
        step = buffer.duration().map(|d| d.mseconds()).unwrap_or(pts.saturating_sub(last).max(step));
        last = pts;
        let map = buffer.map_readable()?;
        stats.push(stat(&map));
        frames.push(map.to_vec());
    }
    let error = pipeline.bus().and_then(|b| b.pop_filtered(&[gst::MessageType::Error]));
    let _ = pipeline.set_state(gst::State::Null);
    if frames.is_empty() {
        let why = error.map(|m| format!("{:?}", m.view())).unwrap_or_else(|| "it gave no picture".into());
        bail!("this file could not be decoded here: {why}. A clip with alpha wants WebM VP8 or VP9, ProRes 4444, QuickTime Animation or PNG in a MOV");
    }
    let duration_ms = if frames.len() == 1 { 0 } else { last + step.max(1) };
    let used_alpha = *alpha.lock() && stats.iter().any(|s| s.solid < 0.995);
    Ok(Measured { stats, frames, duration_ms, alpha: used_alpha })
}

/// One AYUV frame's numbers.
fn stat(px: &[u8]) -> Stat {
    let n = (px.len() / 4).max(1) as f64;
    let (mut solid, mut white, mut black, mut colour) = (0u32, 0u32, 0u32, 0u64);
    for p in px.chunks_exact(4) {
        solid += (p[0] >= 200) as u32;
        white += (p[1] >= 200 && p[0] >= 200) as u32;
        black += (p[1] <= 30 || p[0] < 16) as u32;
        colour += ((p[2] as i32 - 128).unsigned_abs() + (p[3] as i32 - 128).unsigned_abs()) as u64;
    }
    Stat { solid: solid as f64 / n, white: white as f64 / n, black: black as f64 / n, colour: colour as f64 / n / 255.0 }
}

/// What a measured file is.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub kind: godwinmix_protocol::fx::FxKind,
    pub blend: godwinmix_protocol::fx::FxBlend,
    pub cut_at_ms: Option<u64>,
    pub coverage: Option<f64>,
    pub transition: bool,
    pub effect: bool,
}

/// Decide what `m` is, by the rules at the top of this file.
pub fn classify(m: &Measured) -> Verdict {
    use godwinmix_protocol::fx::{FxBlend, FxKind};
    let grey = m.stats.iter().all(|s| s.colour < 0.03);
    if m.still() || (grey && !m.alpha && m.stats.iter().all(|s| s.black < 0.7)) {
        return Verdict { kind: FxKind::Matte, blend: FxBlend::Normal, cut_at_ms: None, coverage: None, transition: true, effect: false };
    }
    let peak = |f: &dyn Fn(&Stat) -> f64| -> (usize, f64) {
        m.stats.iter().enumerate().map(|(i, s)| (i, f(s))).fold((0, -1.0), |a, b| if b.1 > a.1 { b } else { a })
    };
    if m.alpha {
        let (best, cover) = peak(&|s| s.solid);
        let mid = plateau(&m.stats, best, &|s| s.solid);
        return Verdict { kind: FxKind::Stinger, blend: FxBlend::Normal, cut_at_ms: Some(m.at_ms(mid)), coverage: Some(cover), transition: true, effect: true };
    }
    let dark = m.stats.iter().map(|s| s.black).sum::<f64>() / m.stats.len() as f64;
    let ends_dark = [m.stats.first(), m.stats.last()].iter().flatten().any(|s| s.black >= 0.6);
    if dark > 0.3 || ends_dark {
        let (whitest, white) = peak(&|s| s.white);
        let best = if white > 0.0 { plateau(&m.stats, whitest, &|s| s.white) } else { peak(&|s| 1.0 - s.black).0 };
        return Verdict { kind: FxKind::Overlay, blend: FxBlend::Screen, cut_at_ms: Some(m.at_ms(best)), coverage: Some(white), transition: white >= 0.5, effect: true };
    }
    Verdict { kind: FxKind::Stinger, blend: FxBlend::Normal, cut_at_ms: Some(m.duration_ms / 2), coverage: Some(1.0), transition: true, effect: false }
}

/// The middle of the run of frames as covered as frame `first`, the most
/// covered: a cut there has the most room either side when a frame lands
/// early or late.
fn plateau(stats: &[Stat], first: usize, f: &dyn Fn(&Stat) -> f64) -> usize {
    let top = f(&stats[first]) - 0.02;
    let last = stats[first..].iter().position(|s| f(s) < top).map(|n| first + n - 1).unwrap_or(stats.len() - 1);
    (first + last) / 2
}
