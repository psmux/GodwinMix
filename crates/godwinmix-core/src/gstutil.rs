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

/// Answer latency queries at this proxy source instead of letting them cross
/// into the pipeline behind it.
///
/// A latency query travels upstream from the mixers, and
/// `gst_pad_query_latency_default` (gstpad.c) fails the whole query if any one
/// sink pad that has a peer cannot answer. Every source branch of the
/// programme's compositor and audio mixer leads, through `proxysrc`, into a
/// separate input pipeline, and that pipeline is regularly in no state to
/// answer: while it is being built, while it is being torn down, and for the
/// whole of `FREEZE_HOLD` when a rebuilt source's branch is kept in the
/// programme with its last frame while its pipeline is stopped.
///
/// One such branch fails the query for the whole programme, and the damage is
/// not the failure itself but what the aggregator does with it. In
/// `gst_aggregator_query_latency_unlocked` (gstaggregator.c) the
/// `min-upstream-latency` this mixer sets deliberately, so that a source
/// attaching later cannot force a pipeline-wide latency recalculation, is
/// folded in only after the query has succeeded; when it fails that line is
/// never reached, `has_peer_latency` stays false, and
/// `gst_aggregator_get_latency_unlocked` hands a force-live aggregator a
/// latency of zero. The mixers then run with no slack at all where they were
/// configured for `MIN_UPSTREAM_LATENCY_NS`, and they re-ask on every pass of
/// the aggregate loop for ever: on air on 2026-09-12 that was 950,278
/// "Latency query failed" warnings from the compositor and 691,333 from the
/// audio mixer in a few hours, which is one per output frame each.
///
/// Two pipelines that are each given the same clock and base time do not have
/// a latency to negotiate across the join, so this answers zero and lets the
/// aggregator raise it to the figure it was configured with. `pad` must be the
/// proxy source's **src** pad, which is where an upstream query passes.
pub fn answer_latency_here(element: &gst::Element) -> Result<()> {
    let pad = element
        .static_pad("src")
        .with_context(|| format!("{} has no src pad", element.name()))?;
    pad.add_probe(gst::PadProbeType::QUERY_UPSTREAM, |_pad, info| {
        let Some(query) = info.query_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        let gst::QueryViewMut::Latency(latency) = query.view_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        // Live, because the programme is; no minimum, because the join adds
        // none; no maximum, because nothing here imposes one.
        latency.set(true, gst::ClockTime::ZERO, gst::ClockTime::NONE);
        gst::PadProbeReturn::Handled
    })
    .context("installing the latency answer on a proxy source")?;
    Ok(())
}

/// Answer this element's downstream negotiation queries at its own src pad
/// instead of letting them travel, and answer the caps one with `caps`.
///
/// For an aggregator inside a source pipeline this is the difference between
/// negotiating in microseconds and stopping dead. `gst_aggregator_default_negotiate`
/// begins with `gst_pad_peer_query_caps (srcpad, template_caps)`, and the
/// template caps of a compositor's src pad are every raw video format there
/// is. From a layered source's compositor that query travels the whole
/// normalising chain, through two converters and a scaler that each expand it
/// again, and then across `proxysink` into the programme pipeline, where it
/// ends on a sink pad of the programme's own compositor. So the source's video
/// negotiation waits on the programme's compositor, which is the one element
/// on the rig that is regularly busy for a long time: adding a pad for a new
/// source, releasing one for a source that has gone.
///
/// Caught in the act on 2026-09-12 on a rig running eight sources with a
/// superimposed page added and removed every round. A source was judged
/// stalled having delivered four frames; its counters said twelve page frames
/// and eighteen decoded video frames had reached its compositor and four had
/// come out, while its audio mixer had produced 224 buffers over the same
/// window. A three second sample of every thread in the process showed exactly
/// one thread inside `gst_aggregator_default_negotiate`, and it was that
/// source's compositor, for 792 of 1803 samples. The five healthy layered
/// compositors were not in it at all. A compositor renegotiates twice in the
/// life of a source, once for each layer, so this was not a loop: it was one
/// caps query that did not come back.
///
/// The chain downstream of these aggregators ends at a capsfilter pinned to
/// the canvas, so there is nothing to negotiate that is not known here
/// already. Answering it here makes the source's video independent of what the
/// programme is doing, which is the property the whole two-pipeline design
/// exists to give and the one place it was not being had.
pub fn answer_negotiation_here(element: &gst::Element, caps: &gst::Caps) -> Result<()> {
    let pad = element
        .static_pad("src")
        .with_context(|| format!("{} has no src pad", element.name()))?;
    let answer = caps.clone();
    pad.add_probe(gst::PadProbeType::QUERY_DOWNSTREAM, move |_pad, info| {
        let Some(query) = info.query_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        match query.view_mut() {
            gst::QueryViewMut::Caps(q) => {
                // Honour the filter the asker sent, as a real peer would: the
                // aggregator passes its own template caps and expects an
                // answer inside them.
                let result = match q.filter() {
                    Some(filter) => {
                        filter.intersect_with_mode(&answer, gst::CapsIntersectMode::First)
                    }
                    None => answer.clone(),
                };
                q.set_result(&result);
                gst::PadProbeReturn::Handled
            }
            // Every successful caps negotiation is followed by
            // `gst_aggregator_do_allocation`, which sends an allocation query
            // down the same road and so across the same proxy. Answered here
            // with nothing on offer, which is what the aggregator already
            // copes with: the query failing outright is "not a problem, just
            // debug a little" in its own words, and this is the same outcome
            // without the wait. Nothing downstream of these aggregators offers
            // a pool worth having anyway; the next element is a plain
            // videoconvert in system memory.
            //
            // Measured over three identical twenty minute runs of the same
            // rig, twenty nine rounds each, counting the times the supervisor
            // judged a source stalled and the times it rebuilt one: 108 and 86
            // with both queries crossing, 64 and 42 with the caps query
            // answered here, 54 and 30 with this one answered too.
            gst::QueryViewMut::Allocation(_) => gst::PadProbeReturn::Handled,
            _ => gst::PadProbeReturn::Ok,
        }
    })
    .context("installing the negotiation answer on an aggregator")?;
    Ok(())
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

/// Who a bus message belongs to.
///
/// The mixer used to work this out by stripping an `input-` or `output-`
/// prefix off a label it had made itself, which meant the attribution of every
/// error depended on a naming convention nothing enforced. The owner is
/// declared when the watcher is installed, so a message can only be attributed
/// to the thing that actually posted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusOwner {
    /// The programme pipeline, the one failure this design cannot absorb.
    Programme,
    Source(String),
    Output(String),
    Multiview,
    /// Anything else with a bus, named for the log.
    Other(String),
}

impl std::fmt::Display for BusOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}

impl BusOwner {
    /// What this owner is called in a log line and a thread name.
    pub fn label(&self) -> String {
        match self {
            Self::Programme => "program".into(),
            Self::Source(id) => format!("input-{id}"),
            Self::Output(id) => format!("output-{id}"),
            Self::Multiview => "multiview".into(),
            Self::Other(name) => name.clone(),
        }
    }

    pub fn source(&self) -> Option<&str> {
        match self {
            Self::Source(id) => Some(id),
            _ => None,
        }
    }

    pub fn output(&self) -> Option<&str> {
        match self {
            Self::Output(id) => Some(id),
            _ => None,
        }
    }
}

/// Messages we care about from a pipeline bus.
#[derive(Debug, Clone)]
pub enum BusEvent {
    Error { pipeline: BusOwner, src: String, message: String, debug: Option<String> },
    Warning { pipeline: BusOwner, src: String, message: String },
    Eos { pipeline: BusOwner },
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

/// Device handles that every pipeline in this process shares rather than each
/// making one of its own.
///
/// An element that needs one asks for it on the bus and, hearing nothing back,
/// makes its own. On this Mac the VideoToolbox decoder made a fresh
/// `GstGLDisplay` for every input pipeline and none of them were ever freed:
/// over 34 add and remove cycles of a superimposed web source there were 31
/// `gldisplay-event` threads still running and four descriptors a cycle that
/// never came back, which is the shape of the descriptor leak seen on air.
/// Answering with the first display created is what GStreamer's own
/// documentation tells an application hosting several pipelines to do.
///
/// Only the GL display, which is documented as shareable and is the one
/// measured here. Anything else a pipeline asks for is left to it.
const SHARED_CONTEXT_TYPES: &[&str] = &["gst.gl.GLDisplay", "gst.gl.app_context"];

fn shared_contexts() -> &'static Mutex<std::collections::HashMap<String, gst::Context>> {
    static CONTEXTS: std::sync::OnceLock<Mutex<std::collections::HashMap<String, gst::Context>>> =
        std::sync::OnceLock::new();
    CONTEXTS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// Answer a pipeline's requests for a shared device handle, and remember the
/// ones it makes for itself.
///
/// A synchronous handler, because it has to be. A `need-context` message is
/// posted from the element's own thread and answered before that call returns;
/// by the time an asynchronous watcher popped it off the queue the element
/// would have given up waiting and made its own. Everything is passed on to
/// the queue afterwards, so the ordinary watcher below sees what it always saw.
fn share_device_contexts(bus: &gst::Bus) {
    bus.set_sync_handler(|_bus, msg| {
        match msg.view() {
            gst::MessageView::NeedContext(need) => {
                let wanted = need.context_type();
                if SHARED_CONTEXT_TYPES.contains(&wanted) {
                    let held = shared_contexts().lock().ok().and_then(|c| c.get(wanted).cloned());
                    if let (Some(ctx), Some(el)) = (
                        held,
                        msg.src().and_then(|s| s.downcast_ref::<gst::Element>()),
                    ) {
                        el.set_context(&ctx);
                    }
                }
            }
            gst::MessageView::HaveContext(have) => {
                let ctx = have.context();
                let kind = ctx.context_type().to_string();
                if SHARED_CONTEXT_TYPES.contains(&kind.as_str()) {
                    if let Ok(mut held) = shared_contexts().lock() {
                        held.entry(kind).or_insert(ctx);
                    }
                }
            }
            _ => {}
        }
        gst::BusSyncReply::Pass
    });
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
    owner: BusOwner,
    tx: tokio::sync::mpsc::UnboundedSender<BusEvent>,
) -> Result<BusWatch> {
    let label = owner.label();
    let bus = pipeline.bus().context("pipeline has no bus")?;
    share_device_contexts(&bus);
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
                        pipeline: owner.clone(),
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
                        pipeline: owner.clone(),
                        src,
                        message: w.error().to_string(),
                    })
                }
                MessageView::Eos(_) => Some(BusEvent::Eos { pipeline: owner.clone() }),
                MessageView::Element(e) => e
                    .structure()
                    .filter(|s| s.name() == "level")
                    // The telemetry probes read the RMS out of the same
                    // message, and only while a client has asked for them.
                    .inspect(|s| {
                        crate::telemetry::note_level(
                            e.src().map(|o| o.name().to_string()).as_deref(),
                            s,
                        )
                    })
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

    /// A proxy source whose other pipeline is not there cannot answer a
    /// latency query, and one unanswerable source pad fails the query for the
    /// whole programme (`gst_pad_query_latency_default`). That is the state
    /// every source is in while it is being built, torn down, or held with its
    /// last frame on air. The answer has to come from this side.
    #[test]
    fn a_latency_query_is_answered_without_the_other_pipeline() {
        init();
        let pipeline = gst::Pipeline::with_name("latency-answer");
        let proxy = make("proxysrc", "la-proxy").unwrap();
        let sink = make("fakesink", "la-sink").unwrap();
        pipeline.add_many([&proxy, &sink]).unwrap();
        proxy.link(&sink).unwrap();
        let src = proxy.static_pad("src").unwrap();

        // As it stands, with nothing on the other side of the proxy.
        let mut query = gst::query::Latency::new();
        let before = src.query(&mut query);

        answer_latency_here(&proxy).unwrap();
        let mut query = gst::query::Latency::new();
        assert!(src.query(&mut query), "the answer should stand in for the other pipeline");
        let (live, min, max) = query.result();
        assert!(live, "the programme is live");
        assert_eq!(min, gst::ClockTime::ZERO, "the join itself adds nothing");
        assert!(max.is_none(), "and imposes no ceiling");
        // Not asserted the other way round: an unconfigured proxysrc answering
        // at all is a detail of that element, and the point of this is that it
        // no longer matters either way.
        let _ = before;
    }

    /// The caps answer has to actually short-circuit the query, or the fix is
    /// a comment. Downstream here says I420 and the answer says AYUV: if the
    /// query still travelled, the answer would be I420.
    #[test]
    fn an_answered_negotiation_query_never_reaches_downstream() {
        init();
        let pipeline = gst::Pipeline::with_name("caps-answer");
        let comp = make_live_aggregator("compositor", "ca-comp").unwrap();
        let downstream = capsfilter(
            "ca-filter",
            &gst::Caps::builder("video/x-raw").field("format", "I420").build(),
        )
        .unwrap();
        let sink = make("fakesink", "ca-sink").unwrap();
        pipeline.add_many([&comp, &downstream, &sink]).unwrap();
        gst::Element::link_many([&comp, &downstream, &sink]).unwrap();

        let answer = gst::Caps::builder("video/x-raw")
            .field("format", "AYUV")
            .field("width", 1280i32)
            .field("height", 720i32)
            .build();
        answer_negotiation_here(&comp, &answer).unwrap();

        let src = comp.static_pad("src").unwrap();
        let got = src.peer_query_caps(None);
        assert_eq!(got, answer, "the query should have been answered here");

        // And a filter is honoured, the way a real peer would honour it: the
        // aggregator sends its whole template and expects an answer inside it.
        let filter = gst::Caps::builder("video/x-raw").field("width", 1280i32).build();
        let got = src.peer_query_caps(Some(&filter));
        assert!(got.is_subset(&filter), "answer {got} is outside the filter {filter}");
        assert!(!got.is_empty(), "the filter and the answer do intersect");

        // And the allocation query that follows every negotiation is answered
        // here too, with nothing on offer.
        let mut alloc = gst::query::Allocation::new(Some(&answer), true);
        assert!(src.peer_query(&mut alloc), "the allocation query should be answered here");
        assert_eq!(alloc.allocation_pools().count(), 0, "nothing was offered, and that is the answer");
    }

    #[test]
    fn dropping_a_bus_watch_stops_delivery() {
        init();
        let pipeline = gst::Pipeline::with_name("watched");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watch = watch_bus(&pipeline, BusOwner::Other("watched".into()), tx).unwrap();

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
