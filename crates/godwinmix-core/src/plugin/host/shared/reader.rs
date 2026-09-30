//! The reading side: `gmxbussrc` in front of the shared normaliser, and a
//! probe that measures what the bus costs this source.
//!
//! The probe runs on the streaming thread, so it does nothing that waits:
//! atomics, and a `try_lock` on the latency samples that skips a sample
//! rather than wait for the one reader of them.

use crate::plugin::kinds::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::plugin::MediaEnds;
use anyhow::{Context, Result};
use godwinmix_framebus::{monotonic_ns, BusName};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

/// Latency samples kept for the percentiles: the last few seconds at 30 fps.
const SAMPLES: usize = 512;
/// A gap longer than this is one worth naming in the log.
const NOTABLE_GAP_NS: u64 = 250_000_000;

/// What one shared source has seen of the bus.
#[derive(Default)]
pub struct Watch {
    frames: AtomicU64,
    last_ns: AtomicU64,
    longest_gap_ns: AtomicU64,
    last_gap_ns: AtomicU64,
    gaps: AtomicU64,
    latency_us: Mutex<Vec<u32>>,
}

impl Watch {
    /// One frame arrived, handed to the owner at `captured_ns`.
    fn frame(&self, captured_ns: Option<u64>) {
        let now = monotonic_ns();
        let last = self.last_ns.swap(now, Relaxed);
        self.frames.fetch_add(1, Relaxed);
        if last != 0 {
            let gap = now.saturating_sub(last);
            self.longest_gap_ns.fetch_max(gap, Relaxed);
            if gap > NOTABLE_GAP_NS {
                self.last_gap_ns.store(gap, Relaxed);
                self.gaps.fetch_add(1, Relaxed);
            }
        }
        if let (Some(at), Some(mut held)) = (captured_ns, self.latency_us.try_lock()) {
            if held.len() >= SAMPLES {
                held.remove(0);
            }
            held.push((now.saturating_sub(at) / 1000).min(u64::from(u32::MAX)) as u32);
        }
    }

    pub fn frames(&self) -> u64 {
        self.frames.load(Relaxed)
    }

    /// Milliseconds since the last frame, or `None` before the first.
    pub fn quiet_ms(&self) -> Option<u64> {
        let last = self.last_ns.load(Relaxed);
        (last != 0).then(|| monotonic_ns().saturating_sub(last) / 1_000_000)
    }

    /// The numbers `call("share")` answers with.
    pub fn report(&self) -> Value {
        let mut lat = self.latency_us.lock().clone();
        lat.sort_unstable();
        let pick = |q: f64| -> Value {
            if lat.is_empty() {
                return Value::Null;
            }
            let i = ((lat.len() - 1) as f64 * q).round() as usize;
            json!(f64::from(lat[i]) / 1000.0)
        };
        json!({
            "frames": self.frames(),
            "longest_gap_ms": self.longest_gap_ns.load(Relaxed) as f64 / 1e6,
            "last_gap_ms": self.last_gap_ns.load(Relaxed) as f64 / 1e6,
            "gaps": self.gaps.load(Relaxed),
            "latency_ms": {"p50": pick(0.5), "p99": pick(0.99), "max": pick(1.0)},
        })
    }
}

/// The source's pipeline: the bus, then the normaliser every source has.
pub fn build(
    ctx: &BuildCtx,
    thumb: bool,
    name: &BusName,
    dir: &Path,
    watch: &Arc<Watch>,
) -> Result<MediaEnds> {
    super::register_elements()?;
    let src = crate::gstutil::make("gmxbussrc", &format!("{}-src-bus", ctx.id))?;
    src.set_property("bus-name", name.to_string());
    src.set_property("bus-dir", dir.display().to_string());
    probe(&src, watch.clone())?;
    let ingest = Ingest::default().with([src.clone()]).livesync(false);
    assemble(ctx, thumb, ingest, |w: &Wiring| {
        src.link(&w.norm.video_entry()).context("linking the frame bus to the normaliser")?;
        w.has_video.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(KindParts::default())
    })
}

fn probe(src: &gst::Element, watch: Arc<Watch>) -> Result<()> {
    let pad = src.static_pad("src").context("gmxbussrc has no src pad")?;
    let caps = gst::Caps::new_empty_simple(godwinmix_framebus::gst::CAPTURED_CAPS);
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        if let Some(buffer) = info.buffer() {
            let at = buffer
                .meta::<gst::ReferenceTimestampMeta>()
                .filter(|m| m.reference().can_intersect(&caps))
                .map(|m| m.timestamp().nseconds());
            watch.frame(at);
        }
        gst::PadProbeReturn::Ok
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watch_counts_frames_and_reports_percentiles() {
        let w = Watch::default();
        assert!(w.quiet_ms().is_none());
        let now = monotonic_ns();
        w.frame(Some(now.saturating_sub(2_000_000)));
        w.frame(Some(now));
        w.frame(None);
        let r = w.report();
        assert_eq!(r["frames"], 3);
        assert!(r["latency_ms"]["max"].as_f64().unwrap() >= 2.0, "{r}");
        assert!(w.quiet_ms().is_some());
    }
}
