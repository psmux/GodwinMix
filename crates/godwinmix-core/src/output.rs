//! One RTMP destination, in its own pipeline, independently supervised.
//!
//! # Why an output gets a whole pipeline to itself
//!
//! The obvious arrangement is a tee branch in the program pipeline ending at
//! `rtmp2sink`. It does not survive contact with a real network. When the sink
//! loses its connection it returns a flow error, and that error travels back up
//! through the queue and the tee into the encoder, and GStreamer tears down the
//! program pipeline's streaming threads. One dead CDN takes the whole broadcast
//! with it.
//!
//! A deep leaky queue does not help. Leaking handles a queue that is *full*; it
//! does nothing about a downstream element that returns an *error*.
//!
//! So each output lives in a separate `GstPipeline`, fed through
//! `proxysink`/`proxysrc`, exactly as the inputs are. A pipeline is the unit of
//! error propagation in GStreamer, so a failing sink can now only take down its
//! own. The program encoder never hears about it.
//!
//! # Where the outage buffer lives
//!
//! The deep leaky queue sits on the *program* side, before the proxy. It has to:
//! reconnecting rebuilds the output pipeline from scratch, and a buffer inside
//! that pipeline would be destroyed along with it. Keeping it upstream means
//! several seconds of already-encoded data survive the reconnect, which is what
//! makes a short network drop invisible to the viewer.
//!
//! Leaking downstream, rather than blocking, is what stops a slow destination
//! from applying backpressure to the encoder that every other output shares.

use crate::config::OutputConfig;
use crate::gstutil::{self, make, BusEvent, BusOwner};
use crate::plugin::output::{Output, OutputCtx};
use crate::state::{safe_uri_label, OutputId, OutputState, OutputStatus};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const RELINK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The currently running output pipeline and the pieces of it we keep hold of.
struct Live {
    pipeline: gst::Pipeline,
    watch: gstutil::BusWatch,
}

pub struct OutputSlot {
    pub cfg: OutputConfig,
    /// Deep leaky queues in the program pipeline. They hold encoded data
    /// across a reconnect, and the video one is where a keyframe request is
    /// injected from.
    feed_video: gst::Element,
    feed_audio: gst::Element,
    /// Tee pads feeding this output, kept so they can be released when it is
    /// removed. A tee that keeps handing out pads nobody releases leaks one per
    /// output that ever existed.
    tee_pads: Vec<(gst::Element, gst::Pad)>,
    /// The program pipeline, so the proxy sinks can be swapped in place.
    program: gst::Pipeline,
    /// Proxy sinks in the program pipeline that the output pipeline attaches
    /// to. Replaced on every reconnect, see `swap_proxy`.
    vproxy: Mutex<gst::Element>,
    aproxy: Mutex<gst::Element>,
    /// The output's own pipeline, its RTMP sink and its bus watcher, replaced
    /// wholesale on reconnect. The watcher is held here so that retiring the
    /// pipeline also silences it; a watcher that outlived its pipeline would
    /// report the pipeline's dying error as a fresh failure.
    pipeline: Mutex<Option<Live>>,
    bus_tx: mpsc::UnboundedSender<BusEvent>,
    generation: AtomicU32,
    reconnects: AtomicU32,
    /// Whether the RTMP handshake has actually completed, refreshed from the
    /// sink's own statistics rather than inferred from data flow.
    connected: AtomicBool,
    failed: AtomicBool,
    /// Consecutive watchdog ticks spent with a nearly full feed queue.
    overfull_ticks: AtomicU32,
    /// Set while a reconnect is pending. A dying connection produces several
    /// bus errors in quick succession, and without this each one would arm its
    /// own reconnect, producing a storm rather than a retry.
    reconnect_armed: AtomicBool,
    /// The destination itself: the muxer, the sink and the honest answer to
    /// whether the far end has accepted us. Everything above this line is the
    /// same for RTMP, SRT and whatever comes next.
    kind: Mutex<Box<dyn Output>>,
    /// What the implementation said about itself at `initialize`.
    manifest: crate::plugin::Manifest,
    capabilities: crate::plugin::CapabilitySet,
}

impl OutputSlot {
    /// Attach a new output to the program pipeline's encoder tees.
    pub fn attach(
        program: &gst::Pipeline,
        video_tee: &gst::Element,
        audio_tee: &gst::Element,
        cfg: &OutputConfig,
        bus_tx: mpsc::UnboundedSender<BusEvent>,
    ) -> Result<Arc<Self>> {
        let id = &cfg.id;
        // As in `InputPipeline::build_kind`: the instance tag on every line
        // below, and the start time for `--startup-report`.
        let _observe = crate::observe::output_span(id);

        let feed_video = gstutil::queue_time(&format!("out-{id}-vq"), cfg.queue_secs, true)?;
        let feed_audio = gstutil::queue_time(&format!("out-{id}-aq"), cfg.queue_secs, true)?;
        let vproxy = make("proxysink", &format!("out-{id}-vproxy"))?;
        let aproxy = make("proxysink", &format!("out-{id}-aproxy"))?;

        program
            .add_many([&feed_video, &feed_audio, &vproxy, &aproxy])
            .context("adding output feed to program pipeline")?;
        gst::Element::link_many([&feed_video, &vproxy]).context("linking video feed")?;
        gst::Element::link_many([&feed_audio, &aproxy]).context("linking audio feed")?;
        let tee_pads = vec![
            (video_tee.clone(), link_tee_to(video_tee, &feed_video).context("linking video tee")?),
            (audio_tee.clone(), link_tee_to(audio_tee, &feed_audio).context("linking audio tee")?),
        ];
        for el in [&feed_video, &feed_audio, &vproxy, &aproxy] {
            el.sync_state_with_parent().ok();
        }

        let (kind, ready) = crate::plugin::output::open(cfg)?;
        let slot = Arc::new(Self {
            cfg: cfg.clone(),
            feed_video,
            feed_audio,
            tee_pads,
            program: program.clone(),
            vproxy: Mutex::new(vproxy),
            aproxy: Mutex::new(aproxy),
            pipeline: Mutex::new(None),
            bus_tx,
            generation: AtomicU32::new(0),
            reconnects: AtomicU32::new(0),
            connected: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            overfull_ticks: AtomicU32::new(0),
            reconnect_armed: AtomicBool::new(false),
            kind: Mutex::new(kind),
            manifest: ready.manifest,
            capabilities: ready.capabilities,
        });
        slot.spin_up(false)?;
        Ok(slot)
    }

    /// Build and start a fresh output pipeline, discarding any previous one.
    fn spin_up(&self, request_keyframe: bool) -> Result<()> {
        let id = &self.cfg.id;
        let gen = self.generation.fetch_add(1, Ordering::SeqCst);

        // Retire the previous pipeline *before* building the replacement. Two
        // pipelines briefly overlapping means two RTMP clients publishing to
        // the same URL, and a server will reject the newcomer, which turns a
        // single reconnect into an endless storm. It also means two proxysrcs
        // competing for one proxysink.
        //
        // The gap costs nothing: the feed queue upstream keeps accepting the
        // encoder's output throughout.
        if let Some(old) = self.pipeline.lock().take() {
            drop(old.watch);
            let _ = old.pipeline.set_state(gst::State::Null);
        }

        // Give this generation a brand new proxy pair.
        //
        // A `proxysrc` attaching to a `proxysink` that already served a
        // previous consumer does not replay the sticky events. The replacement
        // muxer then receives buffers with no StreamStart, no Segment and,
        // fatally, no Caps, so it cannot know the video codec and writes an
        // unknown one into the FLV header. Servers reject that outright
        // ("unsupported video codec: 15") and the output can never reconnect.
        //
        // A freshly linked `proxysink` gets the sticky events resent to it by
        // the queue upstream, which is ordinary pad behaviour and does not
        // depend on the proxy elements cooperating. The queues themselves stay
        // put, so the buffered seconds survive the swap.
        if gen > 0 {
            self.swap_proxy(&self.feed_video, &self.vproxy, format!("out-{id}-vproxy-{gen}"))?;
            self.swap_proxy(&self.feed_audio, &self.aproxy, format!("out-{id}-aproxy-{gen}"))?;
        }

        let pipeline = gst::Pipeline::with_name(&format!("output-{id}"));
        crate::observe::register_pipeline(&format!("output-{id}"), &pipeline);

        let vsrc = make("proxysrc", &format!("out-{id}-vsrc-{gen}"))?;
        vsrc.set_property("proxysink", &*self.vproxy.lock());
        let asrc = make("proxysrc", &format!("out-{id}-asrc-{gen}"))?;
        asrc.set_property("proxysink", &*self.aproxy.lock());
        // Short queues here only give each branch its own thread; the buffer
        // that matters lives upstream in the program pipeline.
        let vq = gstutil::queue_thread(&format!("out-{id}-mux-vq-{gen}"))?;
        let aq = gstutil::queue_thread(&format!("out-{id}-mux-aq-{gen}"))?;

        pipeline
            .add_many([&vsrc, &vq, &asrc, &aq])
            .context("adding output elements")?;
        gst::Element::link_many([&vsrc, &vq]).context("linking output video")?;
        gst::Element::link_many([&asrc, &aq]).context("linking output audio")?;
        hold_audio_until_video_caps(&vq, &aq, id)?;
        // Everything above this line is the same for every destination. The
        // muxer and the sink are the destination's own, and this is the only
        // place the core hands over.
        let params = self.cfg.effective_params();
        self.kind
            .lock()
            .build(
                &OutputCtx {
                    id,
                    generation: gen,
                    pipeline: &pipeline,
                    params: &params,
                    cfg: &self.cfg,
                },
                &vq,
                &aq,
            )
            .with_context(|| format!("building the {} half of output {id}", self.manifest.provide_id()))?;

        self.connected.store(false, Ordering::Relaxed);

        let watch = gstutil::watch_bus(&pipeline, BusOwner::Output(id.clone()), self.bus_tx.clone())
            .context("watching output bus")?;
        pipeline.set_state(gst::State::Playing).context("starting output pipeline")?;

        *self.pipeline.lock() = Some(Live { pipeline, watch });

        // Ask the encoder for a keyframe. Without it the freshly connected
        // server has nothing decodable until the next scheduled one, which at a
        // two second GOP means up to two seconds of black for anyone joining.
        //
        // The event is injected on the program side: upstream events do not
        // cross a proxysrc/proxysink boundary.
        if request_keyframe {
            if let Some(pad) = self.feed_video.static_pad("src") {
                gstutil::force_keyframe(&pad);
            }
        }
        Ok(())
    }

    /// Replace the proxysink feeding one branch, keeping the queue in front of
    /// it untouched so its buffered data survives.
    fn swap_proxy(
        &self,
        feed: &gst::Element,
        slot: &Mutex<gst::Element>,
        name: String,
    ) -> Result<()> {
        let fresh = make("proxysink", &name)?;
        self.program.add(&fresh).context("adding replacement proxysink")?;
        fresh.sync_state_with_parent().ok();

        let src = feed.static_pad("src").context("feed queue has no src pad")?;
        let old = slot.lock().clone();

        let (relink_src, relink_old, relink_new) = (src.clone(), old.clone(), fresh.clone());
        gstutil::with_pad_blocked(&src, RELINK_TIMEOUT, move || {
            if let Some(p) = relink_old.static_pad("sink") {
                let _ = relink_src.unlink(&p);
            }
            if let Some(p) = relink_new.static_pad("sink") {
                if let Err(e) = relink_src.link(&p) {
                    warn!(?e, "failed to link feed queue to replacement proxysink");
                }
            }
        })
        .context("swapping proxysink while blocked")?;

        let _ = old.set_state(gst::State::Null);
        let _ = self.program.remove(&old);
        *slot.lock() = fresh;
        Ok(())
    }

    /// Claim the right to schedule a reconnect.
    ///
    /// Returns false if one is already pending, so the several bus errors a
    /// dying connection emits collapse into a single retry.
    pub fn try_arm_reconnect(&self) -> bool {
        self.reconnect_armed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Rebuild the output pipeline. The program pipeline is untouched.
    pub fn reconnect(&self) -> Result<()> {
        let n = self.reconnects.fetch_add(1, Ordering::SeqCst) + 1;
        info!(output = %self.cfg.id, attempt = n, "reconnecting output");
        self.overfull_ticks.store(0, Ordering::Relaxed);
        let result = self.spin_up(true);
        // Released whether or not it worked: a failed spin-up re-arms through
        // the normal error path with the next backoff step.
        self.reconnect_armed.store(false, Ordering::SeqCst);
        result?;
        self.failed.store(false, Ordering::Relaxed);
        Ok(())
    }

    /// Bus messages from this output's pipeline carry this label.
    pub fn owns_pipeline(&self, owner: &BusOwner) -> bool {
        owner.output() == Some(self.cfg.id.as_str())
    }

    pub fn mark_failed(&self) {
        self.failed.store(true, Ordering::Relaxed);
        self.connected.store(false, Ordering::Relaxed);
    }

    /// True once the RTMP handshake has completed on the current pipeline.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed) && !self.failed.load(Ordering::Relaxed)
    }

    /// Re-read the sink's own statistics to find out whether we are genuinely
    /// connected, and report the transition.
    ///
    /// The obvious signal, a buffer reaching the sink, is wrong: `rtmp2sink`
    /// runs with `async-connect`, so it accepts buffers immediately and
    /// performs the handshake in the background. Data flowing therefore says
    /// nothing about whether the far end ever answered, and an operator would
    /// see "live" against a destination that was refusing us.
    ///
    /// Liveness is the implementation's own answer. It used to be a reading of
    /// `rtmp2sink`'s `stats.out-chunk-size`, which no other sink has; an output
    /// that cannot answer that question can still answer this one.
    pub fn refresh_connected(&self) {
        let now = self.pipeline.lock().is_some() && self.kind.lock().connected();
        if now != self.connected.swap(now, Ordering::Relaxed) {
            if now {
                info!(output = %self.cfg.id, kind = %self.manifest.provide_id(), "output connection established");
            } else {
                warn!(output = %self.cfg.id, kind = %self.manifest.provide_id(), "output connection lost");
            }
        }
    }

    /// What this output is, as a plugin qualified id.
    pub fn type_id(&self) -> String {
        self.manifest.provide_id()
    }

    /// What the core may assume about this output.
    pub fn capabilities(&self) -> crate::plugin::CapabilitySet {
        self.capabilities
    }

    /// A sustained near-full feed queue means the destination cannot keep up.
    /// Falling further behind forever is worse than a reconnect, so past a
    /// threshold we tell the caller to rebuild the connection.
    pub fn tick_overflow_watchdog(&self, ticks_before_reconnect: u32) -> bool {
        let level = gstutil::queue_level_secs(&self.feed_video);
        if level > self.cfg.queue_secs * 0.9 {
            let n = self.overfull_ticks.fetch_add(1, Ordering::Relaxed) + 1;
            if n >= ticks_before_reconnect {
                warn!(
                    output = %self.cfg.id,
                    level_secs = level,
                    "output queue has been full for too long, forcing a reconnect"
                );
                self.overfull_ticks.store(0, Ordering::Relaxed);
                return true;
            }
        } else {
            self.overfull_ticks.store(0, Ordering::Relaxed);
        }
        false
    }

    pub fn state(&self) -> OutputState {
        if self.connected.load(Ordering::Relaxed) {
            OutputState::Live
        } else if self.failed.load(Ordering::Relaxed) {
            OutputState::Reconnecting
        } else if self.reconnects.load(Ordering::Relaxed) > 0 {
            OutputState::Reconnecting
        } else {
            OutputState::Connecting
        }
    }

    pub fn status(&self) -> OutputStatus {
        OutputStatus {
            id: self.cfg.id.clone(),
            uri_host: safe_uri_label(&self.cfg.uri),
            state: self.state(),
            reconnects: self.reconnects.load(Ordering::Relaxed),
            queue_secs: gstutil::queue_level_secs(&self.feed_video),
            // Per kind data, for an output built by a plugin rather than by
            // the core. Nothing the core builds itself has any.
            extra: Default::default(),
        }
    }

    pub fn id(&self) -> &OutputId {
        &self.cfg.id
    }

    pub fn shutdown(&self) {
        if let Some(live) = self.pipeline.lock().take() {
            drop(live.watch);
            let _ = live.pipeline.set_state(gst::State::Null);
        }
    }

    /// Remove this output from the program pipeline entirely.
    ///
    /// Unlike `shutdown`, which only stops the sink, this also takes the feed
    /// queues and proxy sinks back out and releases the tee pads, so an output
    /// removed at runtime leaves nothing behind.
    pub fn detach(&self, program: &gst::Pipeline) {
        self.shutdown();
        for (el, pad) in [
            (&self.feed_video, "feed video"),
            (&self.feed_audio, "feed audio"),
        ]
        .map(|(e, n)| (e.clone(), n))
        {
            let _ = el.set_state(gst::State::Null);
            if let Err(e) = program.remove(&el) {
                warn!(output = %self.cfg.id, part = pad, ?e, "could not remove feed element");
            }
        }
        for proxy in [&self.vproxy, &self.aproxy] {
            let el = proxy.lock().clone();
            let _ = el.set_state(gst::State::Null);
            let _ = program.remove(&el);
        }
        for (tee, pad) in &self.tee_pads {
            tee.release_request_pad(pad);
        }
        info!(output = %self.cfg.id, "output detached");
    }
}

/// Stop audio reaching the muxer until video caps have arrived.
///
/// A reconnecting output joins an already-running encoder mid-stream, and its
/// two proxy branches wake up independently. If audio gets there first,
/// `flvmux` writes its FLV header before it knows what the video codec is and
/// emits an extended/unknown video codec id. Servers reject the connection
/// outright ("unsupported video codec: 15"), so the output reconnects, loses
/// the race again, and never recovers.
///
/// Holding audio until the video caps event has passed makes the ordering
/// deterministic. This cannot deadlock: the program pipeline always has a
/// slate on the compositor, so video is always flowing whether or not any
/// camera is alive.
fn hold_audio_until_video_caps(
    video_queue: &gst::Element,
    audio_queue: &gst::Element,
    id: &str,
) -> Result<()> {
    let apad = audio_queue.static_pad("src").context("audio queue has no src pad")?;
    let vpad = video_queue.static_pad("src").context("video queue has no src pad")?;

    let Some(block) = apad.add_probe(gst::PadProbeType::BLOCK_DOWNSTREAM, |_p, _i| {
        gst::PadProbeReturn::Ok
    }) else {
        // Already released, nothing to coordinate.
        return Ok(());
    };

    let held = Arc::new(Mutex::new(Some((apad.clone(), block))));
    let held_for_buffer = held.clone();
    let oid = id.to_string();
    vpad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_p, info| {
        let Some(gst::PadProbeData::Event(e)) = &info.data else {
            return gst::PadProbeReturn::Ok;
        };
        if !matches!(e.view(), gst::EventView::Caps(_)) {
            return gst::PadProbeReturn::Ok;
        }
        if let Some((pad, id)) = held.lock().take() {
            pad.remove_probe(id);
            debug!(output = %oid, "video caps seen, releasing audio into the muxer");
        }
        gst::PadProbeReturn::Remove
    });

    // Safety net. Caps always precede buffers, so this should never be what
    // releases the hold; it exists so that an unexpected event order degrades
    // to a bad FLV header rather than to a silently stalled output.
    let fallback = held_for_buffer;
    let oid = id.to_string();
    vpad.add_probe(gst::PadProbeType::BUFFER, move |_p, _i| {
        if let Some((pad, id)) = fallback.lock().take() {
            pad.remove_probe(id);
            warn!(output = %oid, "video buffer arrived before caps, releasing audio anyway");
        }
        gst::PadProbeReturn::Remove
    });
    Ok(())
}

fn link_tee_to(tee: &gst::Element, dest: &gst::Element) -> Result<gst::Pad> {
    let src = tee.request_pad_simple("src_%u").context("tee refused a new src pad")?;
    let sink = dest.static_pad("sink").context("destination has no sink pad")?;
    src.link(&sink).context("linking tee branch")?;
    Ok(src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OutputPolicy, ReconnectConfig};

    fn init() {
        let _ = gst::init();
    }

    fn harness() -> (gst::Pipeline, gst::Element, gst::Element, mpsc::UnboundedSender<BusEvent>) {
        let pipeline = gst::Pipeline::with_name("test-program");
        let vtee = make("tee", "vtee").unwrap();
        vtee.set_property("allow-not-linked", true);
        let atee = make("tee", "atee").unwrap();
        atee.set_property("allow-not-linked", true);
        pipeline.add_many([&vtee, &atee]).unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        (pipeline, vtee, atee, tx)
    }

    fn cfg(id: &str) -> OutputConfig {
        OutputConfig::bare(id, &format!("rtmp://127.0.0.1:1935/live/{id}"))
    }

    #[test]
    fn an_output_lives_in_its_own_pipeline() {
        init();
        let (program, vtee, atee, tx) = harness();
        let slot = OutputSlot::attach(&program, &vtee, &atee, &cfg("primary"), tx).unwrap();

        // The isolation guarantee: nothing belonging to the sink may sit in the
        // program pipeline, or its flow errors would tear the program down.
        let names: Vec<String> = program
            .iterate_recurse()
            .into_iter()
            .flatten()
            .map(|e| e.name().to_string())
            .collect();
        assert!(
            !names.iter().any(|n| n.contains("rtmp") || n.contains("mux")),
            "sink elements leaked into the program pipeline: {names:?}"
        );
        // The outage buffer, by contrast, must be program-side so it survives a
        // reconnect that destroys the output pipeline.
        assert!(names.iter().any(|n| n == "out-primary-vq"));
        assert_eq!(slot.state(), OutputState::Connecting);

        slot.shutdown();
        let _ = program.set_state(gst::State::Null);
    }

    #[test]
    fn reconnecting_replaces_the_pipeline_and_leaves_the_feed_intact() {
        init();
        let (program, vtee, atee, tx) = harness();
        let slot = OutputSlot::attach(&program, &vtee, &atee, &cfg("primary"), tx).unwrap();
        let feed_linked = || slot.feed_video.static_pad("src").unwrap().is_linked();
        assert!(feed_linked());

        // A long broadcast reconnects many times; every cycle must leave the
        // graph usable and must not accumulate pipelines.
        for expected in 1..=5u32 {
            slot.reconnect().unwrap();
            assert_eq!(slot.status().reconnects, expected);
            assert!(feed_linked(), "feed came unlinked after reconnect {expected}");
            assert!(slot.pipeline.lock().is_some());
        }
        slot.shutdown();
        assert!(slot.pipeline.lock().is_none());
        let _ = program.set_state(gst::State::Null);
    }

    #[test]
    fn bus_labels_are_attributed_to_the_right_output() {
        init();
        let (program, vtee, atee, tx) = harness();
        let a = OutputSlot::attach(&program, &vtee, &atee, &cfg("primary"), tx.clone()).unwrap();
        let b = OutputSlot::attach(&program, &vtee, &atee, &cfg("backup"), tx).unwrap();

        assert!(a.owns_pipeline(&BusOwner::Output("primary".into())));
        assert!(!a.owns_pipeline(&BusOwner::Output("backup".into())));
        assert!(b.owns_pipeline(&BusOwner::Output("backup".into())));
        // Must not claim the program's own errors, which are unrecoverable and
        // have to be reported rather than silently retried.
        assert!(!a.owns_pipeline(&BusOwner::Programme));
        assert!(!b.owns_pipeline(&BusOwner::Multiview));

        a.shutdown();
        b.shutdown();
        let _ = program.set_state(gst::State::Null);
    }

    #[test]
    fn an_unconnected_output_never_reports_itself_live() {
        init();
        let (program, vtee, atee, tx) = harness();
        // Nothing is listening on this port, so the handshake cannot complete.
        let mut c = cfg("nowhere");
        c.uri = "rtmp://127.0.0.1:1/live/nowhere".into();
        let slot = OutputSlot::attach(&program, &vtee, &atee, &c, tx).unwrap();

        for _ in 0..5 {
            slot.refresh_connected();
            assert!(!slot.is_connected(), "reported connected with no server present");
            assert_ne!(slot.state(), OutputState::Live);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        slot.shutdown();
        let _ = program.set_state(gst::State::Null);
    }

    #[test]
    fn reconnects_do_not_stack_up() {
        init();
        let (program, vtee, atee, tx) = harness();
        let slot = OutputSlot::attach(&program, &vtee, &atee, &cfg("primary"), tx).unwrap();

        // A dying connection emits several bus errors. Only the first may arm
        // a retry, or the retries pile into a storm.
        assert!(slot.try_arm_reconnect(), "first error should arm a reconnect");
        assert!(!slot.try_arm_reconnect(), "second error must not arm another");
        assert!(!slot.try_arm_reconnect());

        slot.reconnect().unwrap();
        // Once the retry has run, the next failure may arm again.
        assert!(slot.try_arm_reconnect());

        slot.shutdown();
        let _ = program.set_state(gst::State::Null);
    }

    #[test]
    fn only_one_output_pipeline_exists_at_a_time() {
        init();
        let (program, vtee, atee, tx) = harness();
        let slot = OutputSlot::attach(&program, &vtee, &atee, &cfg("primary"), tx).unwrap();

        // Overlapping pipelines would mean two RTMP clients publishing to the
        // same URL. The old one must be gone before the new one starts.
        for _ in 0..4 {
            let before = slot.pipeline.lock().as_ref().map(|l| l.pipeline.clone());
            slot.reconnect().unwrap();
            let after = slot.pipeline.lock().as_ref().map(|l| l.pipeline.clone());
            let before = before.expect("a pipeline should have existed");
            let after = after.expect("a pipeline should exist after reconnect");
            assert_ne!(before, after, "reconnect did not replace the pipeline");
            assert_eq!(
                before.current_state(),
                gst::State::Null,
                "the previous output pipeline was left running"
            );
        }
        slot.shutdown();
        let _ = program.set_state(gst::State::Null);
    }

    #[test]
    fn an_idle_queue_never_trips_the_overflow_watchdog() {
        init();
        let (program, vtee, atee, tx) = harness();
        let slot = OutputSlot::attach(&program, &vtee, &atee, &cfg("primary"), tx).unwrap();
        for _ in 0..20 {
            assert!(!slot.tick_overflow_watchdog(3));
        }
        slot.shutdown();
        let _ = program.set_state(gst::State::Null);
    }

    #[test]
    fn policy_presets_reach_the_slot() {
        let mut c = cfg("cdn-out");
        c.policy = OutputPolicy::Cdn;
        assert_eq!(c.reconnect_policy().initial_delay_ms, 1000);
        c.reconnect = Some(ReconnectConfig {
            initial_delay_ms: 42,
            max_delay_ms: 100,
            multiplier: 1.0,
        });
        assert_eq!(c.reconnect_policy().initial_delay_ms, 42);
    }
}
