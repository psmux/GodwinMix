//! The program pipeline and the actor that owns it.
//!
//! # Why the encoder is the fixed point
//!
//! An RTMP output has to look like one unbroken stream: monotonic timestamps,
//! no gaps, and codec parameters that never change. So the encoder starts once
//! and runs until the broadcast ends. Everything that changes during a
//! broadcast happens upstream of it, in raw video and raw audio, where
//! switching source is a property change on a compositor pad and the encoder
//! cannot tell that anything happened.
//!
//! That single decision is what makes a take free:
//!
//! * No element is added, removed or restarted.
//! * No caps are renegotiated, because every source was normalised to the
//!   canvas contract before it got here.
//! * The RTMP connection is not touched, so nothing downstream reconnects.
//!
//! # Why nothing can stall it
//!
//! The compositor and audiomixer are built with `force-live`, so they emit
//! black and silence on schedule even with every input dead. Sources live in
//! their own pipelines behind `proxysrc`, so a camera that errors cannot post
//! a bus message here. The slate sits at the bottom of the compositor's z
//! order, permanently, so losing a source reveals black rather than freezing
//! on its last frame.

use crate::caps::CanvasCaps;
use crate::config::{Config, OutputConfig, SourceConfig};
use crate::gstutil::{self, make, BusEvent};
use crate::input::{MediaReport, InputPipeline, SourceKind};
use crate::multiview::Multiview;
use crate::output::OutputSlot;
use crate::probe::Backends;
use crate::state::*;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, error, info, warn};

/// How often the supervisor checks source liveness and output queue depth.
const TICK: Duration = Duration::from_millis(500);
/// Consecutive ticks with a near-full output queue before forcing a reconnect.
const OVERFLOW_TICKS: u32 = 6;
/// A source that has been stalled this long gets its pipeline rebuilt.
const RESTART_AFTER_STALL: Duration = Duration::from_secs(10);
/// How long to wait for a brand new source to deliver anything before trying
/// the other RTMP client implementation. Long enough to cover a slow handshake,
/// short enough that an operator is not left staring at "connecting".
const CLIENT_FALLBACK_AFTER: Duration = Duration::from_secs(6);
/// Source id used for the ad break. Reserved: a configured source may not use it.
pub const AD_ID: &str = "__ad__";
/// How long to wait for an ad to preroll before taking it anyway.
const AD_PREROLL: Duration = Duration::from_millis(1200);
/// Upstream latency the mixers assume from the start, so that attaching a
/// source later does not force a pipeline-wide latency recalculation.
const MIN_UPSTREAM_LATENCY_NS: i64 = 500_000_000;
/// Lead-in between rolling an ad and cutting to it.
///
/// The ad's first frame has to be *due* slightly in the future, not right now.
/// Timestamped for the current instant it would arrive a few milliseconds late,
/// and the mixer discards late frames: an eight second ad lost its first 1.4
/// seconds that way. Cutting at the same moment the first frame falls due costs
/// nothing visible and plays the file whole.
const AD_LEAD_IN: gst::ClockTime = gst::ClockTime::from_mseconds(500);

/// Reply channel for a command that a caller is waiting on.
///
/// Without one the control plane can only answer "queued", so a rejected
/// request, a duplicate source id or a missing ad file, looks like success to
/// whoever clicked the button.
pub type Ack = oneshot::Sender<Result<(), String>>;

fn reply(ack: Option<Ack>, outcome: &Result<()>) {
    if let Some(tx) = ack {
        let _ = tx.send(outcome.as_ref().map(|_| ()).map_err(|e| format!("{e:#}")));
    }
}

pub enum Command {
    /// Put a source on program. `None` cuts to the slate.
    Take {
        source: Option<SourceId>,
        at_running_time_ms: Option<u64>,
        ack: Option<Ack>,
    },
    /// Interrupt the programme with an ad, then return to live.
    ///
    /// `return_to` defaults to whatever is on program when the break starts.
    /// There is no time shift buffer: the source keeps running behind the ad
    /// and we rejoin it live, so the content that played during the break is
    /// not shown.
    AdBreak {
        uri: String,
        at_running_time_ms: Option<u64>,
        return_to: Option<SourceId>,
        ack: Option<Ack>,
    },
    /// Cut the ad short and go back early.
    EndAdBreak(Option<Ack>),
    AddSource(Box<SourceConfig>, Option<Ack>),
    /// A source whose page has been probed for its media, coming back to be
    /// built. Sent by the probe thread `begin_add_source` starts; never by the
    /// API. The report is None when the page had nothing to hand over.
    AddSourceProbed(Box<SourceConfig>, Option<MediaReport>, Option<Ack>),
    RemoveSource(SourceId, Option<Ack>),
    ReconnectOutput(OutputId, Option<Ack>),
    AddOutput(Box<OutputConfig>, Option<Ack>),
    RemoveOutput(OutputId, Option<Ack>),
    RestartSource(SourceId),
    Status(oneshot::Sender<MixerStatus>),
    Bus(BusEvent),
    Tick,
    Shutdown,
}

#[derive(Clone)]
pub struct MixerHandle {
    tx: mpsc::UnboundedSender<Command>,
    events: broadcast::Sender<Event>,
}

impl MixerHandle {
    pub fn send(&self, cmd: Command) -> Result<()> {
        self.tx.send(cmd).map_err(|_| anyhow::anyhow!("mixer is not running"))
    }

    /// Send a command and wait for the mixer to accept or reject it.
    pub async fn request(&self, make: impl FnOnce(Ack) -> Command) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(make(tx))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("mixer dropped the request"))?
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn status(&self) -> Result<MixerStatus> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Status(tx))?;
        rx.await.map_err(|_| anyhow::anyhow!("mixer dropped the status request"))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
}

struct SourceSlot {
    input: InputPipeline,
    /// Compositor and audiomixer pads in the program pipeline.
    vpad: gst::Pad,
    apad: gst::Pad,
    branch: Vec<gst::Element>,
    /// Ticks spent stalled, used to decide when to rebuild the pipeline.
    stalled_ticks: u32,
    /// Ticks since this source was started while it has produced nothing.
    silent_ticks: u32,
    /// Maps this source's running time onto the programme's. `None` for an ad,
    /// which sets its own offset from its cue.
    aligner: Option<Arc<TimelineAligner>>,
    /// Held here rather than in a shared list so that removing the source also
    /// silences its bus. A watcher outliving its pipeline keeps reporting a
    /// dead source's errors forever.
    _watch: gstutil::BusWatch,
}

/// Shifts a source's timeline onto the programme's.
///
/// Every input lives in its own pipeline, so its segment starts when that
/// source starts and its buffers carry running times beginning near zero. The
/// programme may be hours in by then. A compositor hides the discrepancy by
/// reusing whatever frame it is holding, so video looks correct; an audiomixer
/// cannot place samples that claim to belong seconds in the past and silently
/// discards every one. Cameras appeared perfectly live and carried no sound.
///
/// The offset is computed from the first buffer to arrive on either branch and
/// applied to both mixer pads, so video and audio keep their relative timing
/// and the source stays in lip sync.
pub struct TimelineAligner {
    /// Shared by both branches so they get an identical shift.
    offset: Arc<Mutex<Option<i64>>>,
    applied: Arc<AtomicBool>,
}

impl TimelineAligner {
    fn install(
        program: &gst::Pipeline,
        video_queue: &gst::Element,
        audio_queue: &gst::Element,
        vpad: &gst::Pad,
        apad: &gst::Pad,
        id: &str,
    ) -> Result<Arc<Self>> {
        let this = Arc::new(Self {
            offset: Arc::new(Mutex::new(None)),
            applied: Arc::new(AtomicBool::new(false)),
        });
        let clock = program.clock();
        let base = program.base_time();

        for (tag, queue) in [("video", video_queue), ("audio", audio_queue)] {
            let pad = queue.static_pad("src").context("queue has no src pad")?;
            let (vpad, apad) = (vpad.clone(), apad.clone());
            let (clock, id, tag) = (clock.clone(), id.to_string(), tag.to_string());
            let shared = this.offset.clone();
            let applied = this.applied.clone();

            pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_p, info| {
                let Some(gst::PadProbeData::Event(e)) = &info.data else {
                    return gst::PadProbeReturn::Ok;
                };
                if !matches!(e.view(), gst::EventView::Segment(_)) {
                    return gst::PadProbeReturn::Ok;
                }
                // Act on the segment, not on the first buffer. A pad offset
                // adjusts the segment as it traverses the pad, so setting it
                // once buffers are already flowing changes nothing: the mixer
                // has long since decided where this source's timeline sits.
                let Some(now) = clock
                    .as_ref()
                    .and_then(|c| c.time().checked_sub(base.unwrap_or(gst::ClockTime::ZERO)))
                else {
                    return gst::PadProbeReturn::Ok;
                };
                // One offset for both pads, taken from whichever segment
                // arrives first, so video and audio keep their relative timing.
                // Hold the guard once. Re-locking inside the match arm would
                // deadlock the streaming thread: the scrutinee's guard lives
                // for the whole match, and the mutex is not reentrant.
                let offset = {
                    let mut guard = shared.lock();
                    match *guard {
                        Some(v) => v,
                        None => {
                            let v = now.nseconds() as i64;
                            *guard = Some(v);
                            info!(
                                source = %id, first_on = %tag,
                                offset_ms = v / 1_000_000,
                                "aligned source timeline onto the programme"
                            );
                            v
                        }
                    }
                };
                vpad.set_offset(offset);
                apad.set_offset(offset);
                applied.store(true, Ordering::Relaxed);
                gst::PadProbeReturn::Ok
            });
        }
        Ok(this)
    }

    /// Recompute on the next buffer. A restarted source begins its running time
    /// again from zero, so the previous offset no longer holds.
    pub fn reset(&self) {
        *self.offset.lock() = None;
        self.applied.store(false, Ordering::Relaxed);
    }
}

pub struct Mixer {
    cfg: Config,
    canvas: CanvasCaps,
    backends: Backends,
    origin: Instant,

    program: gst::Pipeline,
    vmix: gst::Element,
    amix: gst::Element,
    venc_tee: gst::Element,
    aenc_tee: gst::Element,
    pgm_video_proxy: gst::Element,

    sources: Vec<SourceSlot>,
    outputs: Vec<Arc<OutputSlot>>,
    multiview: Option<Multiview>,
    program_source: Option<SourceId>,
    ad: Option<AdStatus>,
    /// Running time an armed break was asked to land on, so the roll can hit
    /// that mark rather than a fixed lead-in after the pipeline happened to be
    /// ready. Preroll can take anywhere from tens of milliseconds upwards.
    ad_cue_ms: Option<u64>,

    /// Bumped on every take so a superseded audio ramp abandons its work
    /// instead of fighting the newer one.
    take_generation: Arc<AtomicU64>,
    pending_take: Option<gst::SingleShotClockId>,
    pending_ad_end: Option<gst::SingleShotClockId>,
    output_attempts: HashMap<OutputId, u32>,
    source_attempts: HashMap<SourceId, u32>,

    handle: MixerHandle,
    events: broadcast::Sender<Event>,
    /// The mixer runs on a plain OS thread, because GStreamer state changes
    /// block and must never sit on a Tokio worker. That thread has no runtime
    /// context of its own, so `tokio::spawn` from it panics. Delayed work
    /// (reconnect backoff, source restarts) goes through this handle instead.
    rt: tokio::runtime::Handle,
    /// Every pipeline we create gets a bus watcher feeding this. Held here so
    /// that a source added mid-broadcast is supervised exactly like one from
    /// the config file.
    bus_tx: mpsc::UnboundedSender<BusEvent>,
    /// Watches for the program and multiview pipelines, which live as long as
    /// the mixer does. Source watches live on their slots so that removing a
    /// source silences it. Held so they keep running.
    watches: Vec<gstutil::BusWatch>,
    /// Where to persist runtime source changes, if anywhere.
    runtime_store: Option<std::path::PathBuf>,
    /// Sources whose page is still being probed, so not yet in `sources`.
    /// They are written to the runtime store with the rest: a shutdown that
    /// came while a superimposed source was still probing left it out of the
    /// store, and it was gone at the next start.
    pending: Vec<SourceConfig>,
    /// Superimposed sources being built again from scratch after a failure,
    /// with whether each was on programme when it failed, to put it back.
    rebuilding: std::collections::HashMap<SourceId, bool>,
}

impl Mixer {
    #[allow(clippy::type_complexity)]
    pub fn build(
        cfg: Config,
    ) -> Result<(
        Self,
        MixerHandle,
        mpsc::UnboundedReceiver<Command>,
        mpsc::UnboundedReceiver<BusEvent>,
    )> {
        let canvas = CanvasCaps::new(&cfg.canvas);
        let backends = Backends::probe(cfg.hardware.decode, cfg.hardware.encode)?;
        backends.apply_decoder_ranks();

        let rt = tokio::runtime::Handle::try_current().context(
            "Mixer::build must be called from inside a Tokio runtime: the mixer \
             thread has none of its own and uses this handle to schedule retries",
        )?;

        let (tx, rx) = mpsc::unbounded_channel();
        let (bus_tx, bus_rx) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(256);
        let handle = MixerHandle { tx, events: events.clone() };

        let program = gst::Pipeline::with_name("program");

        // --- video: mix, encode, fan out --------------------------------
        let vmix = gstutil::make_live_aggregator("compositor", "vmix")?;
        vmix.set_property_from_str("background", "black");
        crate::probe::set_bool(&vmix, "ignore-inactive-pads", true);
        // Claim a fixed upstream latency up front.
        //
        // Attaching a branch to a running aggregator otherwise makes the
        // pipeline recalculate its latency, and everything downstream pauses
        // while it settles. Rolling an ad cost about a second of output that
        // way. Declaring the figure in advance means a later arrival changes
        // nothing.
        crate::probe::set_int(&vmix, "min-upstream-latency", MIN_UPSTREAM_LATENCY_NS);

        let vmix_caps = gstutil::capsfilter("vmix-caps", &canvas.video())?;
        let vraw_tee = make("tee", "vraw-tee")?;
        vraw_tee.set_property("allow-not-linked", true);

        let venc_q = gstutil::queue_thread("venc-q")?;

        // Whatever the encoder wants, from the canvas's I420. x264enc takes I420 and

        // this passes it through untouched; nvh264enc takes NV12 and RGB formats

        // only, and without this the programme failed to link on the first machine

        // with an NVIDIA GPU. One 720p conversion per frame is the cost, and only

        // where an encoder needs it.

        let venc_conv = make("videoconvert", "venc-conv")?;
        let venc = make(backends.video_encode.element, "venc")?;
        crate::probe::configure_video_encoder(
            &venc,
            backends.video_encode.accel,
            cfg.program.video_bitrate_kbps,
            (cfg.canvas.fps as u32) * cfg.program.keyframe_interval_secs,
        );
        let vparse = make("h264parse", "vparse")?;
        crate::probe::set_int(&vparse, "config-interval", -1);
        // Hold the video back by the AAC encoder's uncompensated delay, so
        // what the viewer hears lines up with what they see. Applied at the
        // encoder's own pad: the raw programme tee for the multiview is not
        // shifted, and the FLV muxer sees both streams already aligned.
        let av_offset_ns = match cfg.program.av_offset_ms {
            Some(ms) => ms * 1_000_000,
            None => {
                let samples = crate::probe::audio_encoder_delay_samples(backends.audio_encode);
                (samples * 1_000_000_000 / cfg.canvas.sample_rate.max(1) as u64) as i64
            }
        };
        if let Some(sink) = venc.static_pad("sink") {
            sink.set_offset(av_offset_ns);
        }
        info!(
            audio_encoder = backends.audio_encode,
            offset_ms = av_offset_ns / 1_000_000,
            "video held back to match the audio encoder's delay"
        );
        let venc_tee = make("tee", "venc-tee")?;
        venc_tee.set_property("allow-not-linked", true);

        // Raw program video for the multiview's return cell.
        let pgm_v_q = gstutil::queue_thread("pgm-v-q")?;
        let pgm_v_scale = make("videoscale", "pgm-v-scale")?;
        let pgm_v_rate = make("videorate", "pgm-v-rate")?;
        let pgm_v_caps = gstutil::capsfilter(
            "pgm-v-caps",
            &CanvasCaps::video_at(
                crate::input::THUMB_WIDTH,
                crate::input::THUMB_HEIGHT,
                gst::Fraction::new(cfg.multiview.fps.max(1), 1),
            ),
        )?;
        let pgm_video_proxy = make("proxysink", "pgm-v-proxy")?;

        // --- audio: mix, encode, fan out --------------------------------
        let amix = gstutil::make_live_aggregator("audiomixer", "amix")?;
        crate::probe::set_bool(&amix, "ignore-inactive-pads", true);
        crate::probe::set_int(&amix, "min-upstream-latency", MIN_UPSTREAM_LATENCY_NS);
        let amix_caps = gstutil::capsfilter("amix-caps", &canvas.audio())?;
        let araw_tee = make("tee", "araw-tee")?;
        araw_tee.set_property("allow-not-linked", true);

        let aenc_q = gstutil::queue_thread("aenc-q")?;
        let aconv = make("audioconvert", "aenc-conv")?;
        let aenc = make(backends.audio_encode, "aenc")?;
        crate::probe::configure_audio_encoder(&aenc, cfg.program.audio_bitrate_kbps);
        let aparse = make("aacparse", "aparse")?;
        let aenc_tee = make("tee", "aenc-tee")?;
        aenc_tee.set_property("allow-not-linked", true);

        // The mosaic carries no audio, so the operator confirms that program
        // has sound from a meter instead. `level` passes audio through
        // untouched and posts peak values on the bus.
        let level = make("level", "pgm-level")?;
        crate::probe::set_bool(&level, "post-messages", true);
        crate::probe::set_int(&level, "interval", 100_000_000);

        // --- slate and silence -------------------------------------------
        // These two pads are never removed and never muted. They are what the
        // program falls back to when no source is selected or the selected one
        // has gone quiet, and they guarantee both mixers always have at least
        // one active pad.
        let slate = make("videotestsrc", "slate")?;
        slate.set_property_from_str("pattern", "black");
        slate.set_property("is-live", true);
        let slate_caps = gstutil::capsfilter("slate-caps", &canvas.video())?;

        let silence = make("audiotestsrc", "silence")?;
        slate.set_property("is-live", true);
        silence.set_property_from_str("wave", "silence");
        silence.set_property("is-live", true);
        let silence_caps = gstutil::capsfilter("silence-caps", &canvas.audio())?;

        program
            .add_many([
                &vmix, &vmix_caps, &vraw_tee, &venc_q, &venc_conv, &venc, &vparse, &venc_tee,
                &pgm_v_q, &pgm_v_rate, &pgm_v_scale, &pgm_v_caps, &pgm_video_proxy,
                &amix, &amix_caps, &level, &araw_tee, &aenc_q, &aconv, &aenc, &aparse, &aenc_tee,
                &slate, &slate_caps, &silence, &silence_caps,
            ])
            .context("adding program elements")?;

        gst::Element::link_many([&vmix, &vmix_caps, &vraw_tee]).context("linking video mixer")?;
        gst::Element::link_many([&vraw_tee, &venc_q, &venc_conv, &venc, &vparse, &venc_tee])
            .context("linking video encoder")?;
        gst::Element::link_many([
            &vraw_tee, &pgm_v_q, &pgm_v_rate, &pgm_v_scale, &pgm_v_caps, &pgm_video_proxy,
        ])
        .context("linking program return video")?;

        gst::Element::link_many([&amix, &amix_caps, &level, &araw_tee])
            .context("linking audio mixer")?;
        gst::Element::link_many([&araw_tee, &aenc_q, &aconv, &aenc, &aparse, &aenc_tee])
            .context("linking audio encoder")?;

        gst::Element::link(&slate, &slate_caps).context("linking slate")?;
        gst::Element::link(&silence, &silence_caps).context("linking silence")?;

        // Slate sits at the bottom of the z order, fully opaque, forever.
        let slate_pad = vmix.request_pad_simple("sink_%u").context("compositor refused slate pad")?;
        slate_pad.set_property("zorder", 0u32);
        slate_pad.set_property("alpha", 1.0f64);
        slate_caps
            .static_pad("src")
            .unwrap()
            .link(&slate_pad)
            .context("linking slate into the mixer")?;

        let silence_pad = amix.request_pad_simple("sink_%u").context("mixer refused silence pad")?;
        silence_pad.set_property("volume", 1.0f64);
        silence_caps
            .static_pad("src")
            .unwrap()
            .link(&silence_pad)
            .context("linking silence into the mixer")?;

        let mixer = Self {
            cfg,
            canvas,
            backends,
            origin: Instant::now(),
            program,
            vmix,
            amix,
            venc_tee,
            aenc_tee,
            pgm_video_proxy,
            sources: Vec::new(),
            outputs: Vec::new(),
            multiview: None,
            program_source: None,
            ad: None,
            ad_cue_ms: None,
            take_generation: Arc::new(AtomicU64::new(0)),
            pending_take: None,
            pending_ad_end: None,
            output_attempts: HashMap::new(),
            source_attempts: HashMap::new(),
            handle: handle.clone(),
            events,
            rt,
            bus_tx,
            watches: Vec::new(),
            runtime_store: None,
            pending: Vec::new(),
            rebuilding: std::collections::HashMap::new(),
        };
        Ok((mixer, handle, rx, bus_rx))
    }

    /// Persist runtime source and output changes to this path.
    pub fn persist_runtime_to(&mut self, path: std::path::PathBuf) {
        self.runtime_store = Some(path);
    }

    /// Bring up multiview, outputs, sources, then start rolling.
    pub fn start(&mut self) -> Result<()> {
        if self.cfg.multiview.enabled {
            let mv = Multiview::build(&self.cfg.multiview, &self.pgm_video_proxy)
                .context("building multiview")?;
            self.watches
                .push(gstutil::watch_bus(mv.pipeline(), "multiview", self.bus_tx.clone())?);
            self.multiview = Some(mv);
        }

        for out in self.cfg.outputs.clone() {
            match OutputSlot::attach(
                &self.program,
                &self.venc_tee,
                &self.aenc_tee,
                &out,
                self.bus_tx.clone(),
            ) {
                Ok(slot) => self.outputs.push(slot),
                Err(e) => error!(output = %out.id, ?e, "failed to attach output"),
            }
        }

        self.watches
            .push(gstutil::watch_bus(&self.program, "program", self.bus_tx.clone())?);
        self.program.set_state(gst::State::Playing).context("starting program pipeline")?;
        if let Some(mv) = &self.multiview {
            mv.start().context("starting multiview")?;
        }

        for src in self.cfg.sources.clone() {
            let id = src.id.clone();
            if let Err(e) = self.begin_add_source(src, None) {
                error!(source = %id, ?e, "failed to add source");
            }
        }

        // Start on the first source if there is one, so a fresh boot is
        // already showing something rather than black.
        let first = self.sources.first().map(|s| s.input.id.clone());
        if first.is_some() {
            self.take(first, None)?;
        } else {
            self.apply_visibility(false);
        }

        info!(
            sources = self.sources.len(),
            outputs = self.outputs.len(),
            "mixer running"
        );
        Ok(())
    }

    // -- sources ---------------------------------------------------------

    /// Start adding a source. Most are built here and now. A website asking
    /// to superimpose is probed first, which means launching a browser and
    /// waiting up to `MEDIA_PROBE_TIMEOUT` for the page to say what it plays,
    /// so that runs on a thread of its own and comes back as
    /// `Command::AddSourceProbed`. This thread answers every other command,
    /// and ten seconds of it standing still would freeze the operator's UI
    /// while the programme carried on underneath.
    fn begin_add_source(&mut self, cfg: SourceConfig, ack: Option<Ack>) -> Result<()> {
        let Some(spec) = InputPipeline::media_probe_spec(&cfg, &self.canvas, &self.cfg.browser)
        else {
            let r = self.add_source(&cfg, None);
            self.finish_rebuild(&cfg, &r);
            reply(ack, &r);
            return r;
        };
        let handle = self.handle.clone();
        let id = cfg.id.clone();
        info!(source = %id, "asking the page what it plays before building the source");
        self.pending.push(cfg.clone());
        let spawned = std::thread::Builder::new()
            .name(format!("probe-{id}"))
            .spawn(move || {
                let report =
                    crate::input::probe_page_media(&id, &spec, crate::input::MEDIA_PROBE_TIMEOUT);
                // A send fails only when the mixer has already gone.
                let _ = handle.send(Command::AddSourceProbed(Box::new(cfg), report, ack));
            });
        match spawned {
            Ok(_) => Ok(()),
            Err(e) => {
                let r = Err(anyhow::Error::from(e).context("spawning the media probe thread"));
                // The ack moved into the closure that never ran; nothing left
                // to answer on. The API gets the error through the log.
                r
            }
        }
    }

    pub fn add_source(&mut self, cfg: &SourceConfig, overlay: Option<MediaReport>) -> Result<()> {
        anyhow::ensure!(cfg.id != AD_ID, "{AD_ID} is reserved for ad breaks");
        anyhow::ensure!(!cfg.id.trim().is_empty(), "a source needs an id");
        anyhow::ensure!(!cfg.uri.trim().is_empty(), "a source needs a uri");
        let kind = SourceKind::detect(&cfg.uri);
        info!(source = %cfg.id, uri = %cfg.uri, ?kind, superimposed = overlay.is_some(), "adding source");
        self.add_source_kind(cfg, kind, true, overlay)?;
        self.persist_runtime();
        Ok(())
    }

    fn add_source_kind(
        &mut self,
        cfg: &SourceConfig,
        kind: SourceKind,
        in_multiview: bool,
        overlay: Option<MediaReport>,
    ) -> Result<()> {
        if self.sources.iter().any(|s| s.input.id == cfg.id) {
            anyhow::bail!("source {} already exists", cfg.id);
        }
        let is_ad = cfg.id == AD_ID;
        let input = InputPipeline::build_kind(
            cfg,
            &self.canvas,
            &self.backends,
            self.cfg.multiview.fps.max(1),
            self.origin,
            kind,
            self.cfg.security.allow_exec_sources,
            &self.cfg.browser,
            overlay,
        )?;

        let id = &cfg.id;
        let vsrc = make("proxysrc", &format!("pgm-vsrc-{id}"))?;
        vsrc.set_property("proxysink", &input.video_proxy);
        let vq = gstutil::queue_thread(&format!("pgm-vq-{id}"))?;
        let asrc = make("proxysrc", &format!("pgm-asrc-{id}"))?;
        asrc.set_property("proxysink", &input.audio_proxy);
        let aq = gstutil::queue_thread(&format!("pgm-aq-{id}"))?;

        let branch = vec![vsrc, vq, asrc, aq];
        self.program.add_many(&branch).context("adding source branch")?;
        gst::Element::link_many([&branch[0], &branch[1]]).context("linking source video")?;
        gst::Element::link_many([&branch[2], &branch[3]]).context("linking source audio")?;

        let vpad = self.vmix.request_pad_simple("sink_%u").context("compositor refused a pad")?;
        vpad.set_property("zorder", 1u32);
        // New sources arrive invisible and silent. Nothing reaches program
        // until an operator asks for it.
        vpad.set_property("alpha", 0.0f64);
        vpad.set_property("xpos", 0i32);
        vpad.set_property("ypos", 0i32);
        vpad.set_property("width", self.canvas.width);
        vpad.set_property("height", self.canvas.height);
        vpad.set_property_from_str("sizing-policy", "keep-aspect-ratio");
        branch[1].static_pad("src").unwrap().link(&vpad).context("linking video into mixer")?;

        let apad = self.amix.request_pad_simple("sink_%u").context("mixer refused a pad")?;
        apad.set_property("volume", 0.0f64);
        branch[3].static_pad("src").unwrap().link(&apad).context("linking audio into mixer")?;

        // Map this source's timeline onto the programme's. Installed before
        // the branch is set running: the segment event travels as soon as data
        // starts, and a probe added afterwards never sees it.
        //
        // Only live sources. An ad sets its own offset from its cue, and two
        // things writing the same pad offset fight: the ad lost a second and a
        // half and opened with a gap.
        //
        // Not a superimposed source either. Its layers are composited on this
        // pipeline's own clock and base time, so what it emits is already at
        // the programme's running time; shifting that by the programme's age
        // again put a source rebuilt two minutes in two minutes into the
        // future, where the compositor's queue held its frames unconsumed,
        // the push into that queue never returned, and every teardown that
        // needed the pad's stream lock afterwards waited on it for good.
        let aligner = if is_ad || input.superimposed() {
            None
        } else {
            Some(TimelineAligner::install(
                &self.program,
                &branch[1],
                &branch[3],
                &vpad,
                &apad,
                &cfg.id,
            )?)
        };

        for el in &branch {
            el.sync_state_with_parent().ok();
        }

        // An ad gets no mosaic tile. It would reshuffle the grid under the
        // operator mid-break, and the program return cell already shows it.
        if in_multiview {
            if let Some(mv) = &mut self.multiview {
                mv.add_tile(Some(cfg.id.clone()), &input.thumb_proxy)
                    .context("adding multiview tile")?;
            }
        }

        let watch =
            gstutil::watch_bus(&input.pipeline, format!("input-{}", cfg.id), self.bus_tx.clone())
                .context("watching input bus")?;
        // Put every source pipeline on the program's clock and base time.
        //
        // Separate pipelines otherwise each pick their own, so running times
        // are not comparable across the proxy boundary. Live RTMP inputs get
        // away with it because their timing comes from arrival, but a file's
        // timestamps start at zero, and a compositor judging them against a
        // programme that has been up for minutes sees them as ancient history.
        if let Some(clock) = self.program.clock() {
            input.pipeline.use_clock(Some(&clock));
            // start-time NONE stops the pipeline resetting base time when it
            // changes state, which would undo the line below.
            input.pipeline.set_start_time(gst::ClockTime::NONE);
            if let Some(base) = self.program.base_time() {
                input.pipeline.set_base_time(base);
            }
        }

        // Register the slot before starting, so that a failure to start can be
        // cleaned up by the ordinary removal path rather than leaking pads and
        // a live pipeline. A leaked failed ad blocked every later break and
        // kept posting its errors.
        self.sources.push(SourceSlot {
            input,
            vpad,
            apad,
            branch,
            stalled_ticks: 0,
            silent_ticks: 0,
            aligner,
            _watch: watch,
        });

        let started = {
            let slot = self.sources.last().expect("slot was just pushed");
            if is_ad {
                // Decode the first frame before the cue so the break does not
                // open on black. A failure here means the file is missing or
                // unreadable, and must abort rather than cutting the programme
                // to black for the ad's whole duration.
                slot.input.preroll(AD_PREROLL).context("preparing media source")
            } else {
                slot.input.start().context("starting input pipeline")
            }
        };
        if let Err(e) = started {
            let _ = self.remove_source(&cfg.id);
            return Err(e);
        }
        info!(source = %cfg.id, "source added");
        self.broadcast_status();
        Ok(())
    }

    pub fn remove_source(&mut self, id: &SourceId) -> Result<()> {
        let slot = self.detach_source(id)?;
        slot.input.stop();
        Ok(())
    }

    /// Build a superimposed source again from nothing, as if it had just been
    /// added: probe the page, fetch its clips, new pipeline.
    ///
    /// The in-place restart that serves every other source does not serve
    /// this one. Brought back in place, the layered pipeline's audio mixer
    /// spun on a failing latency query and its compositor managed under a
    /// frame a second, and the stall restart that followed blocked this thread
    /// for minutes inside the teardown. Building from scratch is the path that
    /// has worked every time, so a failure takes it. The source comes back on
    /// programme if it was there; the programme shows the slate meanwhile, as
    /// for any dead source.
    fn rebuild_source(&mut self, id: &SourceId) {
        let Some(slot) = self.sources.iter().find(|s| &s.input.id == id) else { return };
        let cfg = slot.input.config.clone();
        let was_program = self.program_source.as_ref() == Some(id);
        // Removed the way the API removes a source: its pipeline stopped first,
        // then its branch taken out of the programme. That order matters. The
        // branch's proxy source shares a stream lock with the thread that
        // pushes this source's frames into the programme; stop the branch
        // while that thread is still pushing and the two wait on each other.
        if let Err(e) = self.remove_source(id) {
            warn!(source = %id, ?e, "could not remove the failed source before building it again");
        }
        info!(source = %id, was_program, "building the superimposed source again from scratch");
        self.rebuilding.insert(id.clone(), was_program);
        if let Err(e) = self.begin_add_source(cfg, None) {
            error!(source = %id, ?e, "could not begin building the source again");
            self.rebuilding.remove(id);
        }
    }

    /// The end of a rebuild: the source is back (or is not), so finish what
    /// `rebuild_source` began. Called wherever an add completes.
    fn finish_rebuild(&mut self, cfg: &SourceConfig, added: &Result<()>) {
        let Some(was_program) = self.rebuilding.remove(&cfg.id) else { return };
        match added {
            Ok(()) => {
                info!(source = %cfg.id, was_program, "source built again");
                if was_program {
                    if let Err(e) = self.take(Some(cfg.id.clone()), None) {
                        warn!(source = %cfg.id, ?e, "rebuilt source could not be put back on programme");
                    }
                }
            }
            Err(e) => {
                // Try again in a while, and keep trying: a browser that will
                // not start now may start later, and the source was wanted.
                warn!(source = %cfg.id, ?e, "building the source again failed; trying once more in 10 seconds");
                self.rebuilding.insert(cfg.id.clone(), was_program);
                let handle = self.handle.clone();
                let again = Box::new(cfg.clone());
                self.rt.spawn(async move {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    let _ = handle.send(Command::AddSource(again, None));
                });
            }
        }
    }

    /// Take a source out of the desk without stopping its pipeline. What
    /// remains is the caller's, to stop here or elsewhere.
    fn detach_source(&mut self, id: &SourceId) -> Result<SourceSlot> {
        let Some(pos) = self.sources.iter().position(|s| &s.input.id == id) else {
            anyhow::bail!("no such source {id}");
        };
        if self.program_source.as_ref() == Some(id) {
            self.take(None, None)?;
        }
        let slot = self.sources.remove(pos);
        if let Some(mv) = &mut self.multiview {
            mv.remove_tile(id).ok();
        }
        for el in &slot.branch {
            let _ = el.set_state(gst::State::Null);
            let _ = self.program.remove(el);
        }
        self.vmix.release_request_pad(&slot.vpad);
        self.amix.release_request_pad(&slot.apad);
        info!(source = %id, "source removed");
        if id != AD_ID {
            self.persist_runtime();
        }
        self.broadcast_status();
        Ok(slot)
    }

    /// Attach a new RTMP destination while the broadcast is running.
    pub fn add_output(&mut self, cfg: &OutputConfig) -> Result<()> {
        anyhow::ensure!(!cfg.id.trim().is_empty(), "an output needs an id");
        anyhow::ensure!(!cfg.uri.trim().is_empty(), "an output needs a uri");
        anyhow::ensure!(
            !self.outputs.iter().any(|o| o.id() == &cfg.id),
            "output {} already exists",
            cfg.id
        );
        let slot = OutputSlot::attach(
            &self.program,
            &self.venc_tee,
            &self.aenc_tee,
            cfg,
            self.bus_tx.clone(),
        )
        .with_context(|| format!("attaching output {}", cfg.id))?;
        self.outputs.push(slot);
        info!(output = %cfg.id, "output added");
        self.persist_runtime();
        self.broadcast_status();
        Ok(())
    }

    /// Detach a destination. The programme and every other output carry on.
    pub fn remove_output(&mut self, id: &OutputId) -> Result<()> {
        let Some(pos) = self.outputs.iter().position(|o| o.id() == id) else {
            anyhow::bail!("no such output {id}");
        };
        let slot = self.outputs.remove(pos);
        slot.detach(&self.program);
        self.output_attempts.remove(id);
        info!(output = %id, "output removed");
        self.persist_runtime();
        self.broadcast_status();
        Ok(())
    }

    /// Write the current source list beside the config file.
    ///
    /// Sources added or removed from the UI have to survive a restart, and
    /// rewriting the operator's own config file would throw away its comments
    /// and layout. Once this sidecar exists it is the authoritative list, which
    /// keeps "where do sources come from" a question with a single answer.
    fn persist_runtime(&self) {
        let Some(path) = &self.runtime_store else { return };
        let mut live: Vec<SourceConfig> = self
            .sources
            .iter()
            .filter(|s| s.input.id != AD_ID)
            .map(|s| s.input.config.clone())
            .collect();
        for p in &self.pending {
            if !live.iter().any(|c| c.id == p.id) {
                live.push(p.clone());
            }
        }

        let outputs: Vec<OutputConfig> =
            self.outputs.iter().map(|o| o.cfg.clone()).collect();

        #[derive(serde::Serialize)]
        struct Stored<'a> {
            sources: &'a [SourceConfig],
            outputs: &'a [OutputConfig],
        }
        let body = match toml::to_string_pretty(&Stored { sources: &live, outputs: &outputs }) {
            Ok(b) => format!(
                "# Sources and outputs managed from the LiveboxMix UI or API.\n\
                 # These lists take precedence over the ones in the config file.\n\
                 # Delete this file to go back to the config file's lists.\n\n{b}"
            ),
            Err(e) => {
                warn!(?e, "could not serialise sources");
                return;
            }
        };
        // Write then rename, so a crash mid-write cannot leave a truncated list.
        let tmp = path.with_extension("toml.tmp");
        if let Err(e) = std::fs::write(&tmp, body).and_then(|_| std::fs::rename(&tmp, path)) {
            warn!(path = %path.display(), ?e, "could not save sources");
        } else {
            debug!(path = %path.display(), count = live.len(), "saved sources");
        }
    }

    // -- taking ----------------------------------------------------------

    /// Put `source` on program, optionally at a specific running time.
    ///
    /// An immediate take lands on the compositor's next output frame. A
    /// scheduled one is armed on the pipeline clock, which gets it onto the
    /// intended frame rather than whenever a control message happened to
    /// arrive.
    pub fn take(&mut self, source: Option<SourceId>, at_running_time_ms: Option<u64>) -> Result<()> {
        if let Some(id) = &source {
            if !self.sources.iter().any(|s| &s.input.id == id) {
                anyhow::bail!("no such source {id}");
            }
        }

        if let Some(ms) = at_running_time_ms {
            return self.schedule_take(source, ms);
        }

        self.program_source = source.clone();
        self.take_generation.fetch_add(1, Ordering::SeqCst);
        self.apply_visibility(true);

        let at = self.running_time().unwrap_or(gst::ClockTime::ZERO);
        info!(source = ?source, at_ms = at.mseconds(), "took source to program");
        let _ = self.events.send(Event::Took {
            source,
            at_running_time_ms: at.mseconds(),
        });
        self.broadcast_status();
        Ok(())
    }

    fn schedule_take(&mut self, source: Option<SourceId>, at_ms: u64) -> Result<()> {
        // Cancel anything already armed. Two pending takes would race.
        if let Some(prev) = self.pending_take.take() {
            prev.unschedule();
        }
        let id = self.schedule_command(
            Command::Take { source, at_running_time_ms: None, ack: None },
            at_ms,
        )?;
        self.pending_take = Some(id);
        info!(at_ms, "take scheduled on the pipeline clock");
        Ok(())
    }

    /// Arm a command on the pipeline clock, so it lands on the intended frame
    /// rather than whenever a control message happened to arrive.
    fn schedule_command(&self, cmd: Command, at_ms: u64) -> Result<gst::SingleShotClockId> {
        let clock = self.program.clock().context("program pipeline has no clock")?;
        let base = self.program.base_time().context("program pipeline has no base time")?;
        let id = clock.new_single_shot_id(base + gst::ClockTime::from_mseconds(at_ms));
        let tx = self.handle.clone();
        let cell = std::sync::Mutex::new(Some(cmd));
        id.wait_async(move |_clock, _time, _id| {
            if let Some(cmd) = cell.lock().expect("schedule mutex poisoned").take() {
                if let Err(e) = tx.send(cmd) {
                    warn!(?e, "scheduled command could not reach the mixer");
                }
            }
        })
        .map_err(|e| anyhow::anyhow!("arming scheduled command: {e:?}"))?;
        Ok(id)
    }

    /// Recompute every pad's alpha and volume from current state.
    ///
    /// Declarative on purpose. The watchdog calls this on every tick, so a
    /// source that stalls while live fades to the slate and comes back on its
    /// own when buffers resume, with no separate code path.
    fn apply_visibility(&self, ramp_audio: bool) {
        let mut targets = Vec::new();
        for slot in &self.sources {
            let is_program = self.program_source.as_ref() == Some(&slot.input.id);
            let healthy = matches!(slot.input.observed_state(), SourceState::Live);
            let on = is_program && healthy;

            slot.vpad.set_property("alpha", if on { 1.0f64 } else { 0.0f64 });
            slot.vpad.set_property("zorder", if is_program { 2u32 } else { 1u32 });
            targets.push((slot.apad.clone(), if on { 1.0f64 } else { 0.0f64 }));
        }

        if ramp_audio && self.cfg.program.audio_ramp_ms > 0 {
            ramp_volumes(
                targets,
                Duration::from_millis(self.cfg.program.audio_ramp_ms),
                self.take_generation.clone(),
            );
        } else {
            for (pad, v) in targets {
                if (pad.property::<f64>("volume") - v).abs() > f64::EPSILON {
                    pad.set_property("volume", v);
                }
            }
        }
    }

    fn running_time(&self) -> Option<gst::ClockTime> {
        let clock = self.program.clock()?;
        let base = self.program.base_time()?;
        clock.time().checked_sub(base)
    }

    // -- ad breaks -------------------------------------------------------

    /// Arm an ad break: build and preroll it, then take it now or at a cue.
    fn start_ad_break(
        &mut self,
        uri: String,
        at_running_time_ms: Option<u64>,
        return_to: Option<SourceId>,
    ) -> Result<()> {
        // A cue in the future only arms a timer. Building the pipeline now and
        // holding it PAUSED until the cue does not work: an ad parked for six
        // seconds rolled to black for its whole duration. Instead the ordinary
        // immediate path runs shortly before the cue, so the window between
        // preroll and roll is always the same short one that is known good.
        if let Some(cue) = at_running_time_ms {
            let now = self.running_time().unwrap_or(gst::ClockTime::ZERO).mseconds();
            let build_at = cue.saturating_sub(AD_LEAD_IN.mseconds() + AD_PREROLL.as_millis() as u64);
            if build_at > now {
                if let Some(prev) = self.pending_ad_end.take() {
                    prev.unschedule();
                }
                info!(cue_ms = cue, build_at_ms = build_at, "ad break armed on the pipeline clock");
                self.ad_cue_ms = Some(cue);
                let id = self.schedule_command(
                    Command::AdBreak { uri, at_running_time_ms: None, return_to, ack: None },
                    build_at,
                )?;
                self.pending_ad_end = Some(id);
                return Ok(());
            }
            warn!(cue_ms = cue, now_ms = now, "ad cue is already past; rolling now");
        }

        if self.ad.is_some() {
            self.end_ad_break().ok();
        }
        // Default to rejoining whatever is on program right now.
        let return_to = return_to.or_else(|| self.program_source.clone());
        if let Some(id) = &return_to {
            anyhow::ensure!(
                self.sources.iter().any(|s| &s.input.id == id),
                "cannot return to unknown source {id}"
            );
        }

        let cfg = SourceConfig {
            id: AD_ID.to_string(),
            name: Some("Ad break".into()),
            uri: crate::input::to_uri(&uri),
            stall_timeout_secs: f64::MAX, // An ad ends with EOS, never a stall.
            rtmp_client: Default::default(),
            superimpose: Default::default(), // An ad is a file, never a page.
        };
        if let Err(e) = self.add_source_kind(&cfg, SourceKind::File, false, None) {
            let _ = self.events.send(Event::Alert {
                severity: Severity::Error,
                message: format!("ad break could not start: {e:#}"),
            });
            return Err(e).context("preparing the ad break");
        }

        self.ad = Some(AdStatus { uri, return_to, on_air: false });
        let _ = self.events.send(Event::AdBreakChanged { ad: self.ad.clone() });

        self.roll_ad()
    }

    /// Start the prerolled ad rolling and cut to it as its first frame falls due.
    fn roll_ad(&mut self) -> Result<()> {
        let now = self.running_time().unwrap_or(gst::ClockTime::ZERO);
        // Land on the requested mark when there is one and it is still far
        // enough ahead to deliver the first frame; otherwise cut a lead-in from
        // now. Recomputing from "now" would put a scheduled break on air early
        // whenever preroll finished faster than its budget.
        let earliest = now + AD_LEAD_IN;
        let cue = self
            .ad_cue_ms
            .take()
            .map(gst::ClockTime::from_mseconds)
            .filter(|c| *c >= earliest)
            .unwrap_or(earliest);
        let Some(slot) = self.sources.iter().find(|s| s.input.id == AD_ID) else {
            anyhow::bail!("no ad is armed");
        };

        // Shift the file's timeline onto the programme's.
        //
        // The ad's buffers are stamped from the start of the file, so without
        // this the mixer would judge them against the current running time,
        // find them minutes stale, and stall while it worked out what to do.
        // The same offset goes on both pads so the ad stays in lip sync.
        slot.vpad.set_offset(cue.nseconds() as i64);
        slot.apad.set_offset(cue.nseconds() as i64);
        debug!(cue_ms = cue.mseconds(), "rebasing the ad onto programme time");

        // The file's own duration, known now that it has prerolled.
        let duration = slot
            .input
            .pipeline
            .query_duration::<gst::ClockTime>()
            .filter(|d| *d > gst::ClockTime::ZERO);

        slot.input.start().context("rolling the ad")?;
        if let Some(ad) = &mut self.ad {
            ad.on_air = true;
        }
        let _ = self.events.send(Event::AdBreakChanged { ad: self.ad.clone() });
        info!(
            cue_ms = cue.mseconds(),
            duration_ms = duration.map(|d| d.mseconds()),
            "ad break rolling, cutting to it on cue"
        );
        self.schedule_take(Some(AD_ID.to_string()), cue.mseconds())?;

        // Return to live on the clock, from the file's duration.
        //
        // Reacting to end-of-stream instead cuts the ad short. EOS reaches the
        // mixer as soon as the ad's own pipeline finishes, while more than a
        // second of already-decoded ad is still in flight through the proxy and
        // the queues, and taking the camera back at that moment throws that
        // tail away. An eight second ad played for 6.6.
        if let Some(d) = duration {
            if let Some(prev) = self.pending_ad_end.take() {
                prev.unschedule();
            }
            let end_at = cue + d;
            match self.schedule_command(Command::EndAdBreak(None), end_at.mseconds()) {
                Ok(id) => self.pending_ad_end = Some(id),
                Err(e) => warn!(?e, "could not arm the ad's return; falling back to EOS"),
            }
        } else {
            warn!("ad duration is unknown, returning to live on end-of-stream instead");
        }
        Ok(())
    }

    /// Return to live and dispose of the ad.
    ///
    /// The source we return to has been running behind the ad the whole time,
    /// so we rejoin it live. Nothing is buffered and nothing is replayed.
    fn end_ad_break(&mut self) -> Result<()> {
        if let Some(prev) = self.pending_ad_end.take() {
            prev.unschedule();
        }
        self.ad_cue_ms = None;
        let Some(ad) = self.ad.take() else { return Ok(()) };
        info!(return_to = ?ad.return_to, "ad break finished, returning to live");
        self.take(ad.return_to.clone(), None).ok();
        if self.sources.iter().any(|s| s.input.id == AD_ID) {
            self.remove_source(&AD_ID.to_string()).ok();
        }
        let _ = self.events.send(Event::AdBreakChanged { ad: None });
        self.broadcast_status();
        Ok(())
    }

    // -- supervision -----------------------------------------------------

    pub fn handle(&mut self, cmd: Command) -> Result<bool> {
        match cmd {
            Command::Take { source, at_running_time_ms, ack } => {
                // A scheduled cue that lands on the ad has to start it playing,
                // not merely reveal a paused pipeline.
                if at_running_time_ms.is_none()
                    && source.as_deref() == Some(AD_ID)
                    && self.ad.as_ref().is_some_and(|a| !a.on_air)
                {
                    let r = self.roll_ad();
                    reply(ack, &r);
                    r?;
                } else {
                    let r = self.take(source, at_running_time_ms);
                    reply(ack, &r);
                    r?;
                }
            }
            Command::AdBreak { uri, at_running_time_ms, return_to, ack } => {
                let r = self.start_ad_break(uri, at_running_time_ms, return_to);
                reply(ack, &r);
                r?;
            }
            Command::EndAdBreak(ack) => {
                let r = self.end_ad_break();
                reply(ack, &r);
                r?;
            }
            Command::AddSource(cfg, ack) => {
                self.begin_add_source(*cfg, ack)?;
            }
            Command::AddSourceProbed(cfg, report, ack) => {
                self.pending.retain(|c| c.id != cfg.id);
                let r = self.add_source(&cfg, report);
                self.finish_rebuild(&cfg, &r);
                reply(ack, &r);
                r?;
            }
            Command::RemoveSource(id, ack) => {
                let r = self.remove_source(&id);
                reply(ack, &r);
                r?;
            }
            Command::RestartSource(id) => {
                if self.sources.iter().any(|s| s.input.id == id && s.input.superimposed()) {
                    self.rebuild_source(&id);
                } else if let Some(slot) = self.sources.iter_mut().find(|s| s.input.id == id) {
                    slot.stalled_ticks = 0;
                    // A restarted source starts counting from zero again.
                    if let Some(a) = &slot.aligner {
                        a.reset();
                    }
                    if let Err(e) = slot.input.restart() {
                        error!(source = %id, ?e, "restart failed");
                    }
                }
            }
            Command::ReconnectOutput(id, ack) => {
                let r = if self.outputs.iter().any(|o| o.id() == &id) {
                    self.reconnect_output(&id);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("no such output {id}"))
                };
                reply(ack, &r);
                r?;
            }
            Command::AddOutput(cfg, ack) => {
                let r = self.add_output(&cfg);
                reply(ack, &r);
                r?;
            }
            Command::RemoveOutput(id, ack) => {
                let r = self.remove_output(&id);
                reply(ack, &r);
                r?;
            }
            Command::Status(reply) => {
                let _ = reply.send(self.status());
            }
            Command::Bus(ev) => self.on_bus(ev),
            Command::Tick => self.tick(),
            Command::Shutdown => return Ok(false),
        }
        Ok(true)
    }

    fn on_bus(&mut self, ev: BusEvent) {
        match ev {
            BusEvent::Error { pipeline, src, message, debug: dbg } => {
                debug!(%pipeline, %src, %message, ?dbg, "bus error");

                // An output failing is expected over a long broadcast. Rebuild
                // just its muxer and sink; the encoder never notices.
                if let Some(out) = self.outputs.iter().find(|o| o.owns_pipeline(&pipeline)).cloned() {
                    // A dying connection emits several errors; only the first
                    // arms a retry.
                    if out.try_arm_reconnect() {
                        out.mark_failed();
                        self.emit_output_state(&out);
                        self.arm_output_reconnect(out.id().clone());
                    }
                    return;
                }

                // A source failing is contained in its own pipeline.
                if let Some(id) = pipeline.strip_prefix("input-") {
                    let id = id.to_string();
                    if let Some(slot) = self.sources.iter().find(|s| s.input.id == id) {
                        slot.input.mark_failed();
                        let _ = self.events.send(Event::SourceStateChanged {
                            source: id.clone(),
                            state: SourceState::Failed,
                        });
                        // Fade it off program immediately if it was live.
                        self.apply_visibility(true);
                        self.arm_source_restart(id);
                    }
                    return;
                }

                // Anything else is on the program pipeline itself, which is
                // the one failure this design cannot absorb. Say so plainly.
                error!(%src, %message, "error on the program pipeline");
                let _ = self.events.send(Event::Alert {
                    severity: Severity::Error,
                    message: format!("program pipeline error from {src}: {message}"),
                });
            }
            BusEvent::Warning { pipeline, src, message } => {
                debug!(%pipeline, %src, %message, "bus warning");
            }
            BusEvent::Level { peak_db } => {
                let _ = self.events.send(Event::AudioLevel { peak_db });
            }
            BusEvent::Eos { pipeline } => {
                if pipeline == format!("input-{AD_ID}") {
                    // When the return is already armed from the file's
                    // duration, let it fire: EOS arrives while the tail of the
                    // ad is still in flight, and acting on it truncates the ad.
                    if self.pending_ad_end.is_some() {
                        debug!("ad reached end of file; the scheduled return will handle it");
                        return;
                    }
                    if let Err(e) = self.end_ad_break() {
                        error!(?e, "failed to return to live after the ad");
                    }
                    return;
                }
                warn!(%pipeline, "end of stream");
                if let Some(id) = pipeline.strip_prefix("input-") {
                    self.arm_source_restart(id.to_string());
                }
            }
        }
    }

    fn tick(&mut self) {
        // Reassert visibility so a stall fades to slate and a recovery fades
        // back, without either needing its own event.
        self.apply_visibility(true);


        let mut restart = Vec::new();
        let fallback_ticks = (CLIENT_FALLBACK_AFTER.as_millis() / TICK.as_millis()) as u32;
        for slot in &mut self.sources {
            match slot.input.observed_state() {
                SourceState::Stalled => {
                    slot.stalled_ticks += 1;
                    let ticks = (RESTART_AFTER_STALL.as_millis() / TICK.as_millis()) as u32;
                    if slot.stalled_ticks == ticks {
                        restart.push(slot.input.id.clone());
                    }
                }
                _ => slot.stalled_ticks = 0,
            }

            // A source that connects but never delivers anything is the
            // signature of an RTMP client that cannot talk to this particular
            // server. It reports no error, so nothing else will catch it.
            if slot.input.never_connected() {
                slot.silent_ticks += 1;
                if slot.silent_ticks == fallback_ticks {
                    match slot.input.try_fallback_client() {
                        Ok(true) => {
                            slot.silent_ticks = 0;
                            let _ = self.events.send(Event::Alert {
                                severity: Severity::Warning,
                                message: format!(
                                    "{} delivered no media; retrying with the other RTMP client",
                                    slot.input.id
                                ),
                            });
                        }
                        Ok(false) => {}
                        Err(e) => warn!(source = %slot.input.id, ?e, "client swap failed"),
                    }
                }
            } else {
                slot.silent_ticks = 0;
            }
        }
        for id in restart {
            warn!(source = %id, "source stalled for too long, rebuilding its pipeline");
            self.arm_source_restart(id);
        }

        for slot in &self.sources {
            if matches!(slot.input.observed_state(), SourceState::Live) {
                self.source_attempts.insert(slot.input.id.clone(), 0);
            }
        }

        // Refresh each output's real connection state, then clear the backoff
        // only for the ones genuinely connected.
        for out in &self.outputs {
            out.refresh_connected();
            if out.is_connected() {
                self.output_attempts.insert(out.id().clone(), 0);
            }
        }

        let overflowing: Vec<_> = self
            .outputs
            .iter()
            .filter(|o| o.tick_overflow_watchdog(OVERFLOW_TICKS))
            .map(|o| o.id().clone())
            .collect();
        for id in overflowing {
            self.reconnect_output(&id);
        }
    }

    fn reconnect_output(&mut self, id: &OutputId) {
        let Some(out) = self.outputs.iter().find(|o| o.id() == id).cloned() else {
            return;
        };
        match out.reconnect() {
            Ok(()) => {
                // Deliberately not resetting the backoff here. Building a
                // pipeline succeeding is not the same as the far end accepting
                // us; the counter is cleared in `tick` once data actually
                // flows, so a destination that keeps refusing us backs off.
            }
            Err(e) => {
                error!(output = %id, ?e, "reconnect failed, will retry");
                out.mark_failed();
                self.arm_output_reconnect(id.clone());
            }
        }
        self.emit_output_state(&out);
    }

    fn arm_output_reconnect(&mut self, id: OutputId) {
        let Some(out) = self.outputs.iter().find(|o| o.id() == &id) else { return };
        let attempt = self.output_attempts.entry(id.clone()).or_insert(0);
        let delay = out.cfg.reconnect_policy().delay_for(*attempt);
        *attempt += 1;
        let handle = self.handle.clone();
        info!(output = %id, ?delay, "scheduling output reconnect");
        self.rt.spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = handle.send(Command::ReconnectOutput(id, None));
        });
    }

    /// Schedule a source restart, backing off as failures repeat.
    ///
    /// A server that has gone away will keep refusing us, so retrying every two
    /// seconds forever is just noise. The delay grows to ten seconds and stays
    /// there until the source comes back.
    fn arm_source_restart(&mut self, id: SourceId) {
        let Some(slot) = self.sources.iter().find(|s| s.input.id == id) else {
            return;
        };
        if !slot.input.try_arm_restart() {
            return;
        }
        let attempt = self.source_attempts.entry(id.clone()).or_insert(0);
        let delay = Duration::from_millis(
            (500.0 * 1.8f64.powi((*attempt).min(8) as i32)).min(10_000.0) as u64,
        );
        *attempt += 1;
        let handle = self.handle.clone();
        debug!(source = %id, ?delay, "scheduling source restart");
        self.rt.spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = handle.send(Command::RestartSource(id));
        });
    }

    fn emit_output_state(&self, out: &OutputSlot) {
        let s = out.status();
        let _ = self.events.send(Event::OutputStateChanged {
            output: s.id,
            state: s.state,
            reconnects: s.reconnects,
        });
    }

    fn broadcast_status(&self) {
        let _ = self.events.send(Event::Status(Box::new(self.status())));
    }

    pub fn status(&self) -> MixerStatus {
        let multiview = self.multiview.as_ref().map(|mv| mv.status());
        let cells: HashMap<&str, u32> = multiview
            .as_ref()
            .map(|s| {
                s.cells
                    .iter()
                    .filter_map(|c| Some((c.source.as_deref()?, c.index)))
                    .collect()
            })
            .unwrap_or_default();

        let sources = self
            .sources
            .iter()
            .map(|s| SourceStatus {
                id: s.input.id.clone(),
                name: s.input.config.display_name().to_string(),
                uri: safe_uri_label(&s.input.config.uri),
                state: s.input.observed_state(),
                has_video: s.input.has_video(),
                has_audio: s.input.has_audio(),
                cell: cells.get(s.input.id.as_str()).copied(),
                video_idle_ms: s.input.health.video_idle_ms(),
                audio_idle_ms: s.input.health.audio_idle_ms(),
                superimposed: s.input.superimposed(),
            })
            .collect();

        MixerStatus {
            program: self.program_source.clone(),
            sources,
            outputs: self.outputs.iter().map(|o| o.status()).collect(),
            multiview: multiview.unwrap_or_else(|| MultiviewStatus {
                enabled: false,
                width: 0,
                height: 0,
                cols: 0,
                rows: 0,
                cells: Vec::new(),
                fps: 0,
            }),
            uptime_secs: self.origin.elapsed().as_secs(),
            running_time_ms: self.running_time().map(|t| t.mseconds()).unwrap_or(0),
            ad: self.ad.clone(),
            backend: BackendInfo {
                video_decoder: self.backends.video_decode.element.to_string(),
                video_encoder: self.backends.video_encode.element.to_string(),
                audio_decoder: self.backends.audio_decode.to_string(),
                audio_encoder: self.backends.audio_encode.to_string(),
                hardware_accelerated: self.backends.video_encode.accel
                    != crate::config::Accel::Software,
            },
        }
    }

    /// The mosaic's frame publisher, if the mosaic is running.
    pub fn multiview_sender(&self) -> Option<broadcast::Sender<Arc<[u8]>>> {
        self.multiview.as_ref().map(|mv| mv.sender())
    }

    pub fn shutdown(&mut self) {
        info!("shutting down mixer");
        for p in [self.pending_take.take(), self.pending_ad_end.take()].into_iter().flatten() {
            p.unschedule();
        }
        for out in &self.outputs {
            out.shutdown();
        }
        for slot in &self.sources {
            slot.input.stop();
        }
        if let Some(mv) = &self.multiview {
            mv.stop();
        }
        let _ = self.program.set_state(gst::State::Null);
    }
}

/// Fade a set of audiomixer pads to their targets.
///
/// A hard jump in gain is audible as a click, so a take crossfades over a
/// couple of hundred milliseconds. If another take happens mid-fade the
/// generation changes and this one gives up rather than fighting it.
fn ramp_volumes(targets: Vec<(gst::Pad, f64)>, duration: Duration, generation: Arc<AtomicU64>) {
    let start: Vec<f64> = targets.iter().map(|(p, _)| p.property::<f64>("volume")).collect();
    if targets
        .iter()
        .zip(&start)
        .all(|((_, want), have)| (want - have).abs() < 1e-6)
    {
        return; // Already where we want to be.
    }

    let mine = generation.load(Ordering::SeqCst);
    let steps = 16u32;
    let step = duration / steps;
    std::thread::Builder::new()
        .name("audio-ramp".into())
        .spawn(move || {
            for i in 1..=steps {
                std::thread::sleep(step);
                if generation.load(Ordering::SeqCst) != mine {
                    return; // Superseded by a newer take.
                }
                let t = i as f64 / steps as f64;
                for ((pad, want), have) in targets.iter().zip(&start) {
                    pad.set_property("volume", have + (want - have) * t);
                }
            }
        })
        .ok();
}

/// Run the mixer on its own thread. GStreamer state changes block, so they
/// must not run on a Tokio worker.
pub fn spawn(
    mut mixer: Mixer,
    mut rx: mpsc::UnboundedReceiver<Command>,
    handle: MixerHandle,
) -> std::thread::JoinHandle<()> {
    let ticker = handle.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK);
        loop {
            interval.tick().await;
            if ticker.send(Command::Tick).is_err() {
                return;
            }
        }
    });

    std::thread::Builder::new()
        .name("mixer".into())
        .spawn(move || {
            while let Some(cmd) = rx.blocking_recv() {
                match mixer.handle(cmd) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(e) => warn!(?e, "command failed"),
                }
            }
            mixer.shutdown();
        })
        .expect("spawning mixer thread")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mixer runs on a plain OS thread with no Tokio context. Arming a
    /// reconnect from there used to call `tokio::spawn` and panic with "there
    /// is no reactor running" the first time an output dropped, taking the
    /// whole mixer down. Delayed work must go through a captured handle.
    #[test]
    fn delayed_work_can_be_armed_from_a_thread_with_no_runtime() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let handle = rt.handle().clone();
        let (tx, mut rx) = mpsc::unbounded_channel::<u8>();

        let worker = std::thread::spawn(move || {
            // Proves the precondition: this thread genuinely has no runtime.
            assert!(
                tokio::runtime::Handle::try_current().is_err(),
                "test thread must not have an ambient runtime, or it proves nothing"
            );
            handle.spawn(async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                let _ = tx.send(7);
            });
        });
        worker.join().unwrap();

        let got = rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(2), rx.recv()).await
        });
        assert_eq!(got.unwrap(), Some(7), "scheduled work never ran");
    }

    #[test]
    fn building_outside_a_runtime_fails_with_a_clear_message() {
        let _ = gst::init();
        let err = Mixer::build(crate::config::Config {
            canvas: Default::default(),
            program: Default::default(),
            multiview: Default::default(),
            control: Default::default(),
            hardware: Default::default(),
            media: Default::default(),
            security: Default::default(),
            browser: Default::default(),
            sources: vec![],
            outputs: vec![],
        })
        .err()
        .expect("must refuse to build without a runtime");
        assert!(
            format!("{err:#}").contains("Tokio runtime"),
            "unhelpful error: {err:#}"
        );
    }
}
