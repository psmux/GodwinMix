//! A clip over the whole programme, by a blend mode: an effect fired on its
//! own, or the clip of a stinger with the cut underneath it.
//!
//! Each programme frame it takes the newest decoded frame whose time has
//! come, measured from when the pass started on the programme's own
//! timeline, and blends it. Taking frames by their time rather than one per
//! programme frame is what keeps a stinger's cover on its cut: a clip that
//! started decoding a few frames late skips the frames it missed instead of
//! running late for its whole length.

use super::player::{Player, Shared};
use crate::overlay::blend::{Draw, Planes, Rect, Source};
use crate::overlay::modes::{self, Mode};
use crate::overlay::pass::Pass;
use gstreamer as gst;
use gstreamer_app as gst_app;
use parking_lot::Mutex;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// A decoded frame and when it falls, from the clip's first frame.
struct Frame {
    buffer: gst::Buffer,
    at: u64,
    stride: usize,
    size: (i32, i32),
}

#[derive(Default)]
struct State {
    /// When the pass started, in programme running time.
    start: Option<u64>,
    first_pts: Option<u64>,
    shown: Option<Frame>,
    next: Option<Frame>,
}

pub struct ClipPass {
    name: String,
    sink: gst_app::AppSink,
    shared: Arc<Shared>,
    mode: Mode,
    opacity: u8,
    /// The latest it may still be drawing, from its start, whatever the
    /// clip does: a decoder that stops sending without an end is let go.
    limit_ns: u64,
    state: Mutex<State>,
}

impl ClipPass {
    /// A pass over `player`'s frames. `start` is the running time it begins
    /// at, or `None` to begin on the first frame it is painted on.
    pub fn new(name: &str, player: &Player, mode: Mode, opacity: f64, start: Option<u64>, length_ms: u64) -> ClipPass {
        ClipPass {
            name: name.to_string(),
            sink: player.sink.clone(),
            shared: player.shared.clone(),
            mode,
            opacity: (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            limit_ns: (length_ms + 2_000) * 1_000_000,
            state: Mutex::new(State { start, ..State::default() }),
        }
    }

    /// The next frame in the sink, without waiting for one.
    fn pull(&self, state: &mut State) -> Option<Frame> {
        let sample = self.sink.try_pull_sample(gst::ClockTime::ZERO)?;
        let info = gstreamer_video::VideoInfo::from_caps(sample.caps()?).ok()?;
        let buffer = sample.buffer_owned()?;
        let pts = buffer.pts().map(|t| t.nseconds()).unwrap_or(0);
        let first = *state.first_pts.get_or_insert(pts);
        let stride = info.stride()[0] as usize;
        Some(Frame { buffer, at: pts.saturating_sub(first), stride, size: (info.width() as i32, info.height() as i32) })
    }

    /// Move `shown` on to the newest frame due at `elapsed`.
    fn advance(&self, state: &mut State, elapsed: u64) {
        loop {
            if state.next.is_none() {
                state.next = self.pull(state);
            }
            match state.next.take() {
                Some(f) if f.at <= elapsed => state.shown = Some(f),
                other => {
                    state.next = other;
                    return;
                }
            }
        }
    }
}

impl Pass for ClipPass {
    fn paint(&self, frame: &mut Planes<'_>, now: u64) {
        if self.finished() {
            return;
        }
        let mut state = self.state.lock();
        let start = *state.start.get_or_insert(now);
        let Some(elapsed) = now.checked_sub(start) else { return };
        self.advance(&mut state, elapsed);
        let drained = state.next.is_none() && self.sink.is_eos();
        if drained || elapsed > self.limit_ns {
            self.shared.done.store(true, Ordering::Release);
            return;
        }
        let Some(f) = state.shown.as_ref() else { return };
        let Ok(map) = f.buffer.map_readable() else { return };
        let (w, h) = f.size;
        if map.len() < f.stride * (h as usize - 1) + w as usize * 4 {
            return;
        }
        let canvas = Rect::new(0, 0, frame.width, frame.height);
        let draw = Draw { window: Rect::new(0, 0, w, h), to: canvas, clip: canvas, alpha: self.opacity };
        modes::draw(frame, &Source { data: &map, stride: f.stride }, &draw, self.mode);
    }

    fn finished(&self) -> bool {
        self.shared.done.load(Ordering::Acquire) || self.shared.failed.load(Ordering::Acquire)
    }

    fn name(&self) -> &str {
        &self.name
    }
}
