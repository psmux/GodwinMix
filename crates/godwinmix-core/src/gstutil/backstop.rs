//! The byte and buffer limits behind every queue the core sizes in time.
//!
//! A queue's time level is the running time of the newest buffer in it less
//! that of the oldest. A buffer with no timestamp moves neither end, and a new
//! segment that puts the sink side behind the src side reads as zero until
//! the src side reaches it. Either way a queue with only `max-size-time` set
//! takes every buffer it is given for as long as the thing below it is not
//! reading. On 2026-10-09 a soak test of 0.3.0 watched a show grow by about
//! 40 MB a second, which is one raw 720p30 stream, to 11.8 GB after its
//! ingest plugin was killed, and every queue on the path of a source's picture
//! was sized in time alone.
//!
//! So each of them also gets a buffer limit and a byte limit, set well above
//! what the time limit holds for the media it carries. While timestamps are
//! sane the time limit is reached first and nothing changes. When they are
//! not, the backstop is reached instead: a leaky queue drops its oldest, a
//! blocking one holds its upstream, and in both cases the memory stops. The
//! byte limit is fitted to the caps when they arrive. Raw video is costed per
//! frame at its frame rate, raw audio per second, and anything else at a rate
//! no encoded stream a mixer takes comes near.
//!
//! When a backstop is what stopped a queue, the queue says so in the log, at
//! most once every ten seconds, with its levels. That line names the element
//! whose timestamps stopped counting.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

const MIB: f64 = 1024.0 * 1024.0;

/// Buffers allowed per second of a queue's time limit: 60 fps video three
/// times over, or 10 ms audio buffers twice over.
pub const BUFFERS_PER_SEC: f64 = 200.0;
/// The fewest buffers a backstop allows, so that a short queue still does its
/// job of handing buffers to another thread.
pub const MIN_BUFFERS: u32 = 16;
/// How many times what its time limit costs a queue of raw media may hold in
/// bytes. Two, so a caps frame rate that is wrong by half still never binds.
pub const RAW_HEADROOM: f64 = 2.0;
/// A second of encoded or unknown media. A 4K stream at 100 Mb/s is 12.5 MB.
pub const ENCODED_BYTES_PER_SEC: f64 = 32.0 * MIB;
/// The least a queue may hold in bytes, whatever it carries.
pub const MIN_BYTES: f64 = 4.0 * MIB;
/// Frames a queue of raw video may always hold, however short its time limit.
const MIN_FRAMES: f64 = 4.0;
/// The frame rate assumed for raw video whose caps do not give one.
const UNKNOWN_FPS: f64 = 60.0;
/// How often one queue may say that its backstop was reached.
const REPORT_EVERY_MS: u64 = 10_000;

/// The buffer limit for a queue that holds `secs` of media.
pub fn buffers_for(secs: f64) -> u32 {
    ((secs.max(0.0) * BUFFERS_PER_SEC).ceil() as u32).max(MIN_BUFFERS)
}

/// The byte limit for a queue that holds `secs` of media with these caps, or
/// of media not yet known.
pub fn bytes_for(caps: Option<&gst::CapsRef>, secs: f64) -> u32 {
    let secs = secs.max(0.0);
    let bytes = match caps.and_then(raw_cost) {
        Some((per_sec, frame)) => (per_sec * secs * RAW_HEADROOM).max(frame * MIN_FRAMES),
        None => ENCODED_BYTES_PER_SEC * secs,
    };
    bytes.max(MIN_BYTES).min(u32::MAX as f64) as u32
}

/// What a second of raw media with these caps costs in bytes, and one frame
/// of it (zero for audio). `None` for anything that is not raw.
fn raw_cost(caps: &gst::CapsRef) -> Option<(f64, f64)> {
    let name = caps.structure(0)?.name();
    if name == "video/x-raw" {
        let info = gstreamer_video::VideoInfo::from_caps(caps).ok()?;
        let fps = info.fps();
        let rate = if fps.numer() > 0 && fps.denom() > 0 {
            fps.numer() as f64 / fps.denom() as f64
        } else {
            UNKNOWN_FPS
        };
        let frame = info.size() as f64;
        return Some((frame * rate, frame));
    }
    if name == "audio/x-raw" {
        let info = gstreamer_audio::AudioInfo::from_caps(caps).ok()?;
        return Some((info.rate() as f64 * info.bpf() as f64, 0.0));
    }
    None
}

/// Give `queue`, which holds `secs`, its buffer and byte backstop, and fit the
/// byte limit to the caps each time they arrive.
///
/// The caps are read on the queue's sink pad as the event goes by. Setting a
/// queue's limit takes its lock for a moment and wakes a thread waiting on a
/// full queue to check again; nothing here waits.
pub fn fit(queue: &gst::Element, secs: f64) {
    queue.set_property("max-size-buffers", buffers_for(secs));
    queue.set_property("max-size-bytes", bytes_for(None, secs));
    let Some(pad) = queue.static_pad("sink") else { return };
    let weak = queue.downgrade();
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        let Some(gst::PadProbeData::Event(event)) = &info.data else {
            return gst::PadProbeReturn::Ok;
        };
        if let gst::EventView::Caps(caps) = event.view() {
            let Some(queue) = weak.upgrade() else { return gst::PadProbeReturn::Remove };
            queue.set_property("max-size-bytes", bytes_for(Some(caps.caps()), secs));
        }
        gst::PadProbeReturn::Ok
    });
}

/// Say in the log when `queue` fills by its bytes or buffers while its time
/// level is short of its time limit: the case the backstop is for.
///
/// `overrun` is emitted on the thread pushing into the queue, with the
/// queue's lock released, so reading its levels here is safe. The check is
/// three property reads; the line is written at most once every ten seconds.
pub fn report(queue: &gst::Element) {
    let last = AtomicU64::new(0);
    queue.connect("overrun", false, move |args| {
        let queue = args.first()?.get::<gst::Element>().ok()?;
        if !by_backstop(&queue) {
            return None;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let before = last.load(Ordering::Relaxed);
        if now.saturating_sub(before) < REPORT_EVERY_MS || last.swap(now, Ordering::Relaxed) != before {
            return None;
        }
        warn!(
            queue = %queue.name(),
            pipeline = %top_name(&queue),
            level_ms = queue.property::<u64>("current-level-time") / 1_000_000,
            limit_ms = queue.property::<u64>("max-size-time") / 1_000_000,
            bytes = queue.property::<u32>("current-level-bytes"),
            buffers = queue.property::<u32>("current-level-buffers"),
            "this queue filled by its byte or buffer backstop with less than its time limit \
             in it, so the timestamps of what it holds have stopped counting. It is holding \
             its upstream or dropping its oldest rather than growing"
        );
        None
    });
}

/// Whether a full queue is full by its bytes or buffers rather than its time.
pub fn by_backstop(queue: &gst::Element) -> bool {
    let time = queue.property::<u64>("current-level-time");
    let limit = queue.property::<u64>("max-size-time");
    limit > 0 && time.saturating_mul(10) < limit.saturating_mul(9)
}

/// The name of the pipeline an element is in, or its own if it is in none.
fn top_name(element: &gst::Element) -> String {
    let mut top: gst::Object = element.clone().upcast();
    while let Some(parent) = top.parent() {
        top = parent;
    }
    top.name().to_string()
}

#[cfg(test)]
#[path = "backstop_tests.rs"]
mod tests;
