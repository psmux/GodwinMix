//! The programme off the FIFO, split into its encoded streams, and handed to
//! whatever RTSP media is playing right now.
//!
//! ```text
//!   FIFO ─pump─► appsrc ─► matroskademux ─┬─► queue ─► parser ─► appsink (video) ──┐
//!                                         └─► queue ─► parser ─► appsink (audio) ──┴─► each media's appsrc
//! ```
//!
//! Nothing is decoded. A media that has just started is handed video from the
//! next keyframe on, so a player never starts on a picture it cannot decode.
//! A media that stops reading fills its own appsrc, which drops, and the
//! programme never waits for it.
//!
//! The programme arrives in bursts (a Matroska cluster at a time), so a
//! buffer keeps the time it was made at, moved onto the media's clock: the
//! first buffer a media is handed fixes the offset for both its streams.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSinkCallbacks, AppSrc};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Video,
    Audio,
}

/// One stream the programme carries, as the demuxer found it.
#[derive(Debug, Clone)]
pub struct Track {
    pub caps: gst::Caps,
    /// The parser to put in front of the payloader, and the payloader.
    pub parse: &'static str,
    pub pay: &'static str,
}

/// Where one media's clock stands against the programme's timestamps: the
/// programme time and the media running time of its first buffer.
pub type Base = Arc<Mutex<Option<(gst::ClockTime, gst::ClockTime)>>>;

struct Reader {
    kind: Kind,
    /// Weak: the media owns its appsrc, and a media that is gone drops out.
    src: glib::WeakRef<AppSrc>,
    need_key: bool,
    base: Base,
}

#[derive(Default)]
pub struct Feed {
    pub video: Mutex<Option<Track>>,
    pub audio: Mutex<Option<Track>>,
    readers: Mutex<Vec<Reader>>,
    frames: std::sync::atomic::AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Feed {
    pub fn track(&self, kind: Kind) -> Option<Track> {
        lock(self.slot(kind)).clone()
    }

    pub fn slot(&self, kind: Kind) -> &Mutex<Option<Track>> {
        match kind {
            Kind::Video => &self.video,
            Kind::Audio => &self.audio,
        }
    }

    /// Frames handed to any media so far, for health.
    pub fn frames(&self) -> u64 {
        self.frames.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// A media's appsrc for `kind`, fed from the next keyframe on. Both
    /// streams of one media share `base`.
    pub fn attach(&self, kind: Kind, src: AppSrc, base: Base) {
        if let Some(t) = self.track(kind) {
            src.set_caps(Some(&t.caps));
        }
        lock(&self.readers).push(Reader { kind, src: src.downgrade(), need_key: kind == Kind::Video, base });
    }

    /// The parsed caps, which are what a media's appsrc is given.
    fn note(&self, kind: Kind, caps: &gst::CapsRef) {
        if let Some(t) = lock(self.slot(kind)).as_mut().filter(|t| t.caps.as_ref() != caps) {
            t.caps = caps.to_owned();
        }
    }

    /// One encoded buffer of `kind`, to every media reading it.
    fn hand(&self, kind: Kind, buffer: &gst::Buffer) {
        let key = !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT);
        let targets: Vec<(AppSrc, Base)> = {
            let mut readers = lock(&self.readers);
            readers.retain(|r| r.src.upgrade().is_some());
            let wanted = readers.iter_mut().filter(|r| r.kind == kind && (key || !r.need_key));
            wanted
                .filter_map(|r| {
                    r.need_key = false;
                    Some((r.src.upgrade()?, r.base.clone()))
                })
                .collect()
        };
        for (src, base) in targets {
            // Full or flushing: that media drops this one, nobody waits.
            let _ = src.push_buffer(retimed(buffer, &src, &base));
        }
        if kind == Kind::Video {
            self.frames.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// The appsink a demuxed stream ends in.
    pub fn sink(self: &Arc<Self>, kind: Kind) -> AppSink {
        let sink = AppSink::builder().sync(false).max_buffers(64).drop(true).build();
        let feed = Arc::downgrade(self);
        let on_sample = move |s: &AppSink| {
            let sample = s.pull_sample().map_err(|_| gst::FlowError::Eos)?;
            if let (Some(feed), Some(buffer)) = (feed.upgrade(), sample.buffer_owned()) {
                if let Some(caps) = sample.caps() {
                    feed.note(kind, caps);
                }
                feed.hand(kind, &buffer);
            }
            Ok(gst::FlowSuccess::Ok)
        };
        sink.set_callbacks(AppSinkCallbacks::builder().new_sample(on_sample).build());
        sink
    }
}

/// `buffer` with its time moved onto the media's running time.
fn retimed(buffer: &gst::Buffer, src: &AppSrc, base: &Base) -> gst::Buffer {
    let mut out = buffer.copy();
    let Some(pts) = buffer.pts() else { return out };
    let (made, ran) = *lock(base).get_or_insert_with(|| (pts, src.current_running_time().unwrap_or(gst::ClockTime::ZERO)));
    let at = ran + pts.saturating_sub(made);
    if let Some(b) = out.get_mut() {
        b.set_pts(at);
        b.set_dts(buffer.dts().map(|d| ran + d.saturating_sub(made)).or(Some(at)));
    }
    out
}
