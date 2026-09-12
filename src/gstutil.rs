//! Small helpers over gstreamer-rs.
//!
//! Nothing here is clever. It exists so that the pipeline modules read as a
//! description of the graph rather than as a wall of error handling.

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{error, warn};

pub fn make(factory: &str, name: &str) -> Result<gst::Element> {
    gst::ElementFactory::make(factory)
        .name(name)
        .build()
        .with_context(|| format!("creating element {factory} (named {name})"))
}

/// Build an aggregator (`compositor`, `audiomixer`) that keeps producing
/// output even when no live source is linked upstream.
///
/// `force-live` is construct-only, so it has to be passed to the builder. This
/// single property is what guarantees the program encoder is never starved: a
/// mixer with every input dead still emits black and silence on schedule, and
/// the outgoing stream carries on.
pub fn make_live_aggregator(factory: &str, name: &str) -> Result<gst::Element> {
    match gst::ElementFactory::make(factory)
        .name(name)
        .property("force-live", true)
        .build()
    {
        Ok(el) => Ok(el),
        Err(e) => {
            // Older plugin versions predate force-live. The mixer still works,
            // but a totally dead set of inputs can stall it, so say so loudly.
            warn!(
                factory,
                ?e,
                "could not construct with force-live; output may stall if every input dies"
            );
            make(factory, name)
        }
    }
}

pub fn capsfilter(name: &str, caps: &gst::Caps) -> Result<gst::Element> {
    let el = make("capsfilter", name)?;
    el.set_property("caps", caps);
    Ok(el)
}

/// A queue sized in time rather than buffers.
///
/// `leaky` matters a great deal on the output path. Every output hangs off a
/// shared tee, so a queue that blocks when full would apply backpressure to the
/// encoder and stall *every* output, not just the slow one. Leaking downstream
/// keeps one bad destination from taking the others with it.
pub fn queue_time(name: &str, seconds: f64, leaky: bool) -> Result<gst::Element> {
    let el = make("queue", name)?;
    el.set_property("max-size-buffers", 0u32);
    el.set_property("max-size-bytes", 0u32);
    el.set_property("max-size-time", (seconds * 1e9) as u64);
    el.set_property_from_str("leaky", if leaky { "downstream" } else { "no" });
    Ok(el)
}

/// A short queue used purely to give a branch its own streaming thread.
pub fn queue_thread(name: &str) -> Result<gst::Element> {
    queue_time(name, 1.0, false)
}

/// Current fill level of a queue, in seconds.
pub fn queue_level_secs(q: &gst::Element) -> f64 {
    q.property::<u64>("current-level-time") as f64 / 1e9
}

/// Run `f` while `pad` is blocked, then unblock.
///
/// This is the standard GStreamer idiom for relinking a live pipeline: install
/// an IDLE probe, do the work inside the callback at a point where no buffer is
/// in flight, and remove the probe to resume. The work must happen inside the
/// callback. Unblocking a pad whose peer has been removed makes the upstream
/// queue fail with `not-linked`, which is why relinking cannot be deferred.
pub fn with_pad_blocked<F>(pad: &gst::Pad, timeout: Duration, f: F) -> Result<()>
where
    F: FnOnce() + Send + 'static,
{
    let (tx, rx) = sync_channel::<()>(1);
    let cell = Mutex::new(Some(f));

    // A `None` return is not a failure. It means the pad was already idle, so
    // the callback ran inline on this thread and removed itself before
    // `add_probe` returned. Either way the channel tells us the work is done.
    let _ = pad.add_probe(gst::PadProbeType::IDLE, move |_pad, _info| {
        if let Some(f) = cell.lock().expect("probe mutex poisoned").take() {
            f();
            let _ = tx.send(());
        }
        gst::PadProbeReturn::Remove
    });

    rx.recv_timeout(timeout)
        .context("timed out waiting for pad to reach an idle point")
}

/// Ask the encoder upstream of `pad` for an immediate keyframe.
///
/// Called right after an output reconnects. Without it the new RTMP connection
/// carries nothing decodable until the next scheduled keyframe, which at a two
/// second GOP means up to two seconds of black for anyone joining.
/// `pad` must be a **src** pad. `gst_pad_send_event` on a src pad travels
/// upstream; on a sink pad it would travel downstream and be rejected, which
/// GStreamer reports only as a runtime warning.
pub fn force_keyframe(pad: &gst::Pad) {
    debug_assert_eq!(
        pad.direction(),
        gst::PadDirection::Src,
        "force_keyframe needs a src pad so the event travels upstream"
    );
    if pad.direction() != gst::PadDirection::Src {
        error!(pad = %pad.name(), "refusing to send a force-keyframe event on a sink pad");
        return;
    }
    let event = gstreamer_video::UpstreamForceKeyUnitEvent::builder()
        .all_headers(true)
        .build();
    if !pad.send_event(event) {
        warn!(pad = %pad.name(), "force-keyframe event was not handled upstream");
    }
}

/// Rewrites the CAPS event passing through `pad` so that every colorimetry
/// field is known.
///
/// Hardware decoders on macOS hand out things like `0:4:0:0`: a matrix but no
/// range. GStreamer's converters treat an unknown range as full range, so a
/// stream that is in fact 16..235 (every H.264 stream without a VUI is, by
/// convention) gets its blacks and whites compressed when it is converted to
/// the canvas, or, worse, sets the compositor's output to "unknown" and
/// stretches the correctly tagged sources next to it. When the range is
/// missing the whole tag is treated as a guess and replaced by the default
/// for the picture size (BT.601 for SD, BT.709 for HD, both limited range);
/// when only the other fields are missing they are filled from that default.
pub fn assume_broadcast_colorimetry(element: &gst::Element, pad: &str) -> Result<()> {
    let pad = element
        .static_pad(pad)
        .with_context(|| format!("{} has no {pad} pad", element.name()))?;
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, |_pad, info| {
        let Some(gst::PadProbeData::Event(ev)) = &info.data else {
            return gst::PadProbeReturn::Ok;
        };
        let gst::EventView::Caps(c) = ev.view() else {
            return gst::PadProbeReturn::Ok;
        };
        if let Some(caps) = completed_colorimetry(c.caps()) {
            info.data = Some(gst::PadProbeData::Event(gst::event::Caps::new(&caps)));
        }
        gst::PadProbeReturn::Ok
    })
    .context("installing colorimetry probe")?;
    Ok(())
}

/// The caps with their colorimetry completed as described above, or `None`
/// when they are not raw video or already fully tagged.
pub fn completed_colorimetry(caps: &gst::CapsRef) -> Option<gst::Caps> {
    use gstreamer_video::{
        VideoColorMatrix, VideoColorPrimaries, VideoColorRange, VideoColorimetry, VideoInfo,
        VideoTransferFunction,
    };
    let s = caps.structure(0)?;
    if s.name() != "video/x-raw" || !s.has_field("colorimetry") {
        return None;
    }
    let info = VideoInfo::from_caps(caps).ok()?;
    let have = info.colorimetry();
    let default = VideoInfo::builder(info.format(), info.width(), info.height())
        .build()
        .ok()?
        .colorimetry();
    let want = if have.range() == VideoColorRange::Unknown {
        default
    } else {
        VideoColorimetry::new(
            have.range(),
            if have.matrix() == VideoColorMatrix::Unknown { default.matrix() } else { have.matrix() },
            if have.transfer() == VideoTransferFunction::Unknown {
                default.transfer()
            } else {
                have.transfer()
            },
            if have.primaries() == VideoColorPrimaries::Unknown {
                default.primaries()
            } else {
                have.primaries()
            },
        )
    };
    if want == have {
        return None;
    }
    let mut caps = caps.copy();
    caps.get_mut()?.set("colorimetry", want.to_string());
    Some(caps)
}

/// Messages we care about from a pipeline bus.
#[derive(Debug, Clone)]
pub enum BusEvent {
    Error { pipeline: String, src: String, message: String, debug: Option<String> },
    Warning { pipeline: String, src: String, message: String },
    Eos { pipeline: String },
    /// Peak level per channel in dBFS, from a `level` element. `src` is that
    /// element's name, which is the only thing in the message that says which
    /// meter it came from: the program's own and one per source all post on the
    /// same bus, and without the name they are indistinguishable.
    Level { src: String, peak_db: Vec<f64> },
}

/// A running bus watcher. Dropping it stops the thread and guarantees no
/// further messages are delivered from that pipeline.
///
/// This matters more than it looks. A retired output pipeline whose watcher
/// kept running would deliver its dying error *after* the replacement was
/// already up, and the mixer would read that as a fresh failure and reconnect
/// again. Each reconnect leaves another orphaned watcher, so the mistake
/// sustains itself: a permanent low-grade reconnect loop that never resolves.
pub struct BusWatch {
    stop: Arc<AtomicBool>,
}

impl Drop for BusWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Watch a pipeline bus on a dedicated thread and forward the interesting
/// messages, until the returned `BusWatch` is dropped.
///
/// Polling rather than `add_watch` deliberately: `add_watch` needs a GLib main
/// loop running on some thread, and introducing one alongside a Tokio runtime
/// buys nothing here.
#[must_use = "dropping the BusWatch immediately stops the watcher"]
pub fn watch_bus(
    pipeline: &gst::Pipeline,
    label: impl Into<String>,
    tx: tokio::sync::mpsc::UnboundedSender<BusEvent>,
) -> Result<BusWatch> {
    let label = label.into();
    let bus = pipeline.bus().context("pipeline has no bus")?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::Builder::new()
        .name(format!("bus-{label}"))
        .spawn(move || loop {
            if flag.load(Ordering::SeqCst) || tx.is_closed() {
                return;
            }
            let Some(msg) = bus.timed_pop(gst::ClockTime::from_mseconds(200)) else {
                continue;
            };
            use gst::MessageView;
            let event = match msg.view() {
                MessageView::Error(e) => {
                    let src = e
                        .src()
                        .map(|s| s.path_string().to_string())
                        .unwrap_or_else(|| "unknown".into());
                    error!(pipeline = %label, %src, error = %e.error(), "pipeline error");
                    Some(BusEvent::Error {
                        pipeline: label.clone(),
                        src,
                        message: e.error().to_string(),
                        debug: e.debug().map(|d| d.to_string()),
                    })
                }
                MessageView::Warning(w) => {
                    let src = w
                        .src()
                        .map(|s| s.path_string().to_string())
                        .unwrap_or_else(|| "unknown".into());
                    warn!(pipeline = %label, %src, warning = %w.error(), "pipeline warning");
                    Some(BusEvent::Warning {
                        pipeline: label.clone(),
                        src,
                        message: w.error().to_string(),
                    })
                }
                MessageView::Eos(_) => Some(BusEvent::Eos { pipeline: label.clone() }),
                MessageView::Element(e) => e
                    .structure()
                    .filter(|s| s.name() == "level")
                    .and_then(parse_level)
                    .map(|peak_db| BusEvent::Level {
                        // The element's own name, not its path. A path carries
                        // the pipeline in front of it, and the names these are
                        // matched against are the ones the elements were built
                        // with. A message with no source object at all cannot
                        // be attributed to anything, and "unknown" matches no
                        // meter, so it is dropped downstream rather than here.
                        src: e
                            .src()
                            .map(|s| s.name().to_string())
                            .unwrap_or_else(|| "unknown".into()),
                        peak_db,
                    }),
                _ => None,
            };
            if let Some(ev) = event {
                // Re-check immediately before sending. Without this a message
                // already in hand when the watcher was stopped would still be
                // delivered, and a stale error is exactly what we are guarding
                // against.
                if flag.load(Ordering::SeqCst) {
                    return;
                }
                if tx.send(ev).is_err() {
                    return;
                }
            }
        })
        .context("spawning bus watcher thread")?;
    Ok(BusWatch { stop })
}

/// Pull the per-channel peak out of a `level` element message.
///
/// The field is a GValueArray of doubles. Read it defensively: this is a
/// cosmetic meter, and a plugin that shapes the message differently must not
/// be able to take the bus watcher down with it.
fn parse_level(s: &gst::StructureRef) -> Option<Vec<f64>> {
    let array = s.get::<glib::ValueArray>("peak").ok()?;
    Some(array.iter().filter_map(|v| v.get::<f64>().ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        let _ = gst::init();
    }

    #[test]
    fn dropping_a_bus_watch_stops_delivery() {
        init();
        let pipeline = gst::Pipeline::with_name("watched");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watch = watch_bus(&pipeline, "watched", tx).unwrap();

        let bus = pipeline.bus().unwrap();
        bus.post(gst::message::Eos::new()).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(matches!(rx.try_recv(), Ok(BusEvent::Eos { .. })), "live watcher should deliver");

        drop(watch);
        std::thread::sleep(Duration::from_millis(400));
        // Anything posted after the watch is gone must never arrive, or a
        // retired pipeline's dying error would trigger a spurious reconnect.
        bus.post(gst::message::Eos::new()).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(rx.try_recv().is_err(), "stopped watcher still delivered a message");

        let _ = pipeline.set_state(gst::State::Null);
    }

    #[test]
    fn queue_time_is_configured_in_nanoseconds() {
        init();
        let q = queue_time("q", 5.0, true).unwrap();
        assert_eq!(q.property::<u64>("max-size-time"), 5_000_000_000);
        assert_eq!(q.property::<u32>("max-size-buffers"), 0);
        assert_eq!(queue_level_secs(&q), 0.0);
    }

    #[test]
    fn blocking_an_idle_pad_runs_the_work_inline() {
        init();
        let q = queue_time("q", 1.0, false).unwrap();
        let pad = q.static_pad("src").unwrap();
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f = flag.clone();
        // An idle pad fires the probe on the calling thread and returns no
        // probe id. That is the success path, not an error.
        with_pad_blocked(&pad, Duration::from_secs(2), move || {
            f.store(true, std::sync::atomic::Ordering::SeqCst);
        })
        .expect("blocking an idle pad must succeed");
        assert!(flag.load(std::sync::atomic::Ordering::SeqCst), "work did not run");
    }

    #[test]
    fn blocking_is_repeatable_on_the_same_pad() {
        init();
        // An output that reconnects many times blocks the same pad each time,
        // so the probe must not leave anything behind.
        let q = queue_time("q", 1.0, false).unwrap();
        let pad = q.static_pad("src").unwrap();
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..5 {
            let c = count.clone();
            with_pad_blocked(&pad, Duration::from_secs(2), move || {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();
        }
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 5);
    }

    fn video_caps(colorimetry: Option<&str>, height: i32) -> gst::Caps {
        let mut b = gst::Caps::builder("video/x-raw")
            .field("format", "I420")
            .field("width", height * 16 / 9)
            .field("height", height)
            .field("framerate", gst::Fraction::new(30, 1));
        if let Some(c) = colorimetry {
            b = b.field("colorimetry", c);
        }
        b.build()
    }

    fn colorimetry_of(caps: &gst::Caps) -> String {
        caps.structure(0).unwrap().get::<String>("colorimetry").unwrap()
    }

    #[test]
    fn unknown_range_becomes_limited_range_default_for_the_size() {
        gst::init().unwrap();
        // What vtdec_hw produced for a 720p H.264 stream without a VUI.
        let hd = completed_colorimetry(&video_caps(Some("0:4:0:0"), 720)).unwrap();
        assert_eq!(colorimetry_of(&hd), "bt709");
        let sd = completed_colorimetry(&video_caps(Some("0:4:0:0"), 576)).unwrap();
        assert_eq!(colorimetry_of(&sd), "bt601");
    }

    #[test]
    fn known_range_keeps_its_fields_and_fills_the_rest() {
        gst::init().unwrap();
        // Limited range, BT.601 matrix, nothing else said: keep the matrix.
        let c = completed_colorimetry(&video_caps(Some("1:4:0:0"), 720)).unwrap();
        assert!(colorimetry_of(&c).starts_with("1:4:"), "{}", colorimetry_of(&c));
        assert!(!colorimetry_of(&c).contains(":0"), "{}", colorimetry_of(&c));
    }

    #[test]
    fn complete_or_absent_tags_are_left_alone() {
        gst::init().unwrap();
        assert!(completed_colorimetry(&video_caps(Some("bt709"), 720)).is_none());
        assert!(completed_colorimetry(&video_caps(Some("bt601"), 480)).is_none());
        assert!(completed_colorimetry(&video_caps(None, 720)).is_none());
        let audio = gst::Caps::builder("audio/x-raw").field("colorimetry", "0:0:0:0").build();
        assert!(completed_colorimetry(&audio).is_none());
    }
}
