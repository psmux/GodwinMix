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
use crate::multiview::{Demand, Multiview, MultiviewHandle};
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
/// How often a seekable source's position is published. Four times a second,
/// which is what it takes for a scrubber to look like it is following the
/// picture; the status snapshot every two seconds is far too coarse to drag
/// against. Nothing is sent for a source that cannot be scrubbed.
const POSITION_TICK: Duration = Duration::from_millis(250);
/// Consecutive ticks with a near-full output queue before forcing a reconnect.
const OVERFLOW_TICKS: u32 = 6;
// How long a source may be stalled before its pipeline is rebuilt, and what
// happens when rebuilding it does not help, are `[stall]` in the config.
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
///
/// Twice the figure a layered source composes behind the clock
/// (`LAYER_LATENCY_NS` in input.rs), and that is not incidental. With the two
/// equal, a superimposed source's blocks reached this mixer with no margin at
/// all, and an audiomixer whose input is even slightly behind its output
/// position trims the head of every block it makes: measured on air on
/// 2026-09-11, a slice of 1 to 6 ms missing from every 10 ms block of a
/// superimposed source's sound, at a fixed phase, for minutes at a time (0.53
/// waveform correlation against the source; 0.985 for a whole page). The
/// extra half second is delay a viewer never notices on HLS and margin that
/// the aggregators need.
const MIN_UPSTREAM_LATENCY_NS: i64 = 1_000_000_000;
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

/// What `Command::Configs` answers with.
#[derive(Debug, Clone, Default)]
pub struct RuntimeConfigs {
    pub sources: Vec<SourceConfig>,
    pub outputs: Vec<OutputConfig>,
}

/// What `Command::SetAudio` answers with.
///
/// Three answers rather than an `Option`, because "there is no such source"
/// and "that source has nothing to balance" send the caller to different
/// places: one is a wrong id, the other is a page whose audio Chromium
/// already mixed. An `Ack` cannot carry this, since it only says yes or no
/// and the levels have to come back with the yes.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioOutcome {
    /// Applied, with the fader, the mute and any balance as they now stand.
    Set(SourceAudioState),
    NoSuchSource,
    /// The request asked for a balance and this source's audio arrives already
    /// mixed. Only the balance earns this: the fader and the mute work on every
    /// source, so a request naming one of those is never refused here.
    NotSuperimposed,
}

/// What `Command::Seek` answers with.
///
/// Shaped like `AudioOutcome` and for the same reason: "there is no such source"
/// and "this source has no position to move to" send the caller somewhere
/// different, and a scrubber that got a quiet 200 from a camera would sit there
/// showing a position nothing is playing.
#[derive(Debug, Clone, PartialEq)]
pub enum SeekOutcome {
    /// Landed, at the position and duration read back off the pipeline.
    Moved(SourcePositionState),
    NoSuchSource,
    /// A live feed, or a source whose place on the programme's timeline comes
    /// from something other than the aligner. The ad break is the second kind:
    /// it is a file, and it is still not ours to move. See `SourceSlot::seekable`.
    NotSeekable,
    /// The pipeline took the request and refused it. Rare, and worth saying out
    /// loud rather than reporting a position the source never moved to.
    Failed(String),
}

/// Does this request ask for something only a superimposed source can give?
///
/// The fader and the mute are elements this mixer owns, one pair per source,
/// so they work on a camera and a file as readily as on a page. The page and
/// media balance is different: those gains live on branches that exist only
/// inside a layered source's own pipeline. Keeping the two apart is what lets
/// an operator pull a camera down without being told the camera is not a
/// website.
fn needs_superimposed(page: Option<f64>, media: &[Option<f64>]) -> bool {
    page.is_some() || media.iter().any(|m| m.is_some())
}

/// The name a source's meter is built with, and the only handle on which level
/// messages get attributed back to a source.
///
/// Kept as a function so the name is written once. Ids may contain hyphens, so
/// nothing may take this apart again by splitting on one: `pgm-alevel-cam-1`
/// read that way names `cam`, which is a different source that may well exist.
/// Attribution compares whole names instead.
fn meter_name(id: &str) -> String {
    format!("pgm-alevel-{id}")
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
    /// Move a source's audio controls: the operator's own fader and mute, which
    /// every source has, and for a superimposed one the balance between its page
    /// sound and the videos drawn under it. Every part is optional and only what
    /// is named moves, so the UI can send one fader without knowing where the
    /// others sit.
    SetAudio {
        source: SourceId,
        gain: Option<f64>,
        muted: Option<bool>,
        page: Option<f64>,
        media: Vec<Option<f64>>,
        reply: oneshot::Sender<AudioOutcome>,
    },
    /// Move a seekable source to a position, in milliseconds from its start.
    Seek {
        source: SourceId,
        position_ms: u64,
        reply: oneshot::Sender<SeekOutcome>,
    },
    Status(oneshot::Sender<MixerStatus>),
    /// The configured sources and outputs with their URLs intact. `Status`
    /// masks those, so anything that must match on a URL asks here.
    Configs(oneshot::Sender<RuntimeConfigs>),
    Bus(BusEvent),
    Tick,
    /// Report where each seekable source has got to. Separate from `Tick`
    /// because it runs four times a second and `Tick` runs twice, and the
    /// supervisor's work has no business being done twice as often to suit a
    /// scrubber.
    PositionTick,
    /// Build or tear down the mosaic. Sent by `MultiviewHandle` when the first
    /// client subscribes or the last one leaves, never by the API. See
    /// `multiview.rs`.
    Multiview(Demand),
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

    /// Move part of a source's audio and get back where it ended up. Like
    /// `status`, this waits on a value rather than on an `Ack`: the answer is
    /// the levels, and "not superimposed" is not a failure the caller should
    /// see as a generic 400.
    pub async fn set_audio(
        &self,
        source: SourceId,
        gain: Option<f64>,
        muted: Option<bool>,
        page: Option<f64>,
        media: Vec<Option<f64>>,
    ) -> Result<AudioOutcome> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::SetAudio { source, gain, muted, page, media, reply: tx })?;
        rx.await.map_err(|_| anyhow::anyhow!("mixer dropped the audio request"))
    }

    /// Move a source and get back where it actually landed. Waits on a value
    /// like `set_audio` does: the answer is a position, and "this is a camera"
    /// is not a failure the caller should see as a generic 400.
    pub async fn seek(&self, source: SourceId, position_ms: u64) -> Result<SeekOutcome> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Seek { source, position_ms, reply: tx })?;
        rx.await.map_err(|_| anyhow::anyhow!("mixer dropped the seek request"))
    }

    pub async fn configs(&self) -> Result<RuntimeConfigs> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Configs(tx))?;
        rx.await.map_err(|_| anyhow::anyhow!("mixer dropped the configs request"))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// Push an event to every connected UI without going through the command
    /// queue. For things that happen outside the mixer thread and need nothing
    /// from it, a media upload or a conversion's progress. The mixer's own
    /// state is untouched, so queueing this behind a take would only make a
    /// background job wait.
    pub fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }
}

struct SourceSlot {
    input: InputPipeline,
    /// Compositor and audiomixer pads in the program pipeline.
    vpad: gst::Pad,
    apad: gst::Pad,
    branch: Vec<gst::Element>,
    /// The operator's fader, `pgm-again-{id}`. Also in `branch`, which is what
    /// adds it to the pipeline and takes it out again; this is a second handle
    /// on the same element so the control path does not have to index into a
    /// list to find it.
    again: gst::Element,
    /// The operator's mute, `pgm-amute-{id}`. Muted through its `mute` property
    /// rather than by zeroing its volume, so the two controls never overwrite
    /// each other's value.
    amute: gst::Element,
    /// The name of this source's `level` element. Level messages carry only the
    /// name of the element that posted them, so this is how one is attributed
    /// back to this source.
    meter: String,
    /// Ticks spent stalled, used to decide when to rebuild the pipeline.
    stalled_ticks: u32,
    /// Whether the first picture out of this source has been written down
    /// (see `Mixer::log_timeline`). Once per source, never again.
    first_reported: bool,
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

/// The loudest a fader can be set to. Matches the ceiling the control plane
/// clamps to, so a request that arrives from somewhere else cannot push a
/// volume element past what the API would have allowed.
const MAX_SOURCE_GAIN: f64 = 10.0;

impl SourceSlot {
    /// Where the fader is now, read off the element. A NaN would have silenced
    /// the element for good, which is why the setter refuses one.
    fn gain(&self) -> f64 {
        self.again.property::<f64>("volume")
    }

    fn set_gain(&self, gain: f64) {
        if gain.is_nan() {
            // A volume element set to NaN goes silent permanently and logs
            // nothing. The control plane already refuses this; belt and braces
            // for any other caller.
            warn!(source = %self.input.id, "ignoring a fader value that is not a number");
            return;
        }
        self.again.set_property("volume", gain.clamp(0.0, MAX_SOURCE_GAIN));
    }

    fn muted(&self) -> bool {
        self.amute.property::<bool>("mute")
    }

    fn set_muted(&self, muted: bool) {
        self.amute.set_property("mute", muted);
    }

    /// Can an operator scrub this source?
    ///
    /// Two conditions, and both are necessary. The pipeline has to say it can be
    /// seeked, which is asked of GStreamer rather than guessed from the URI. And
    /// this mixer has to be the thing that places the source on the programme's
    /// timeline, which is what the aligner does. Without an aligner a seek would
    /// restart the source's segment with nothing to work out where the result
    /// belongs, so the ad, whose offset comes from its cue, reports itself as not
    /// seekable and is refused. A superimposed page has no aligner either, and is
    /// not seekable in the first place.
    fn seekable(&self) -> bool {
        self.aligner.is_some() && self.input.seekable()
    }

    /// Where this source has got to, for a source that can say. `None` covers
    /// both a live feed and a file whose pipeline has not started yet.
    fn position(&self) -> Option<SourcePositionState> {
        if !self.seekable() {
            return None;
        }
        Some(SourcePositionState {
            position_ms: self.input.position_ms()?,
            duration_ms: self.input.duration_ms(),
        })
    }

    /// The fader and the mute as they stand, plus the balance if this source has
    /// one. Read off the elements, so a clamped request reports the value that
    /// took effect rather than the one that was asked for.
    fn audio_state(&self) -> SourceAudioState {
        let balance = self.input.levels().map(|l| l.report());
        SourceAudioState {
            gain: self.gain(),
            muted: self.muted(),
            page: balance.as_ref().map(|b| b.page),
            media: balance.map(|b| b.media),
        }
    }
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
    id: String,
    /// Shared by both branches so they get an identical shift.
    offset: Mutex<Option<i64>>,
    applied: AtomicBool,
    /// The pads the shift is applied to, held so that `place_at` is the one
    /// place that writes an offset and can be exercised without a pipeline.
    vpad: gst::Pad,
    apad: gst::Pad,
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
            id: id.to_string(),
            offset: Mutex::new(None),
            applied: AtomicBool::new(false),
            vpad: vpad.clone(),
            apad: apad.clone(),
        });
        let clock = program.clock();
        let base = program.base_time();

        for (tag, queue) in [("video", video_queue), ("audio", audio_queue)] {
            let pad = queue.static_pad("src").context("queue has no src pad")?;
            let (clock, tag) = (clock.clone(), tag.to_string());
            let aligner = this.clone();

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
                aligner.place_at(now.nseconds() as i64, &tag);
                gst::PadProbeReturn::Ok
            });
        }
        Ok(this)
    }

    /// Put this source at `now`, the programme's running time when a segment
    /// arrived on one of its branches, and answer with the offset in force.
    ///
    /// The first segment after a reset decides the offset and every later one
    /// reuses it, so video and audio get an identical shift and the source stays
    /// in lip sync.
    fn place_at(&self, now: i64, first_on: &str) -> i64 {
        // Hold the guard once. Re-locking inside the match arm would deadlock
        // the streaming thread: the scrutinee's guard lives for the whole match,
        // and the mutex is not reentrant.
        let offset = {
            let mut guard = self.offset.lock();
            match *guard {
                Some(v) => v,
                None => {
                    *guard = Some(now);
                    info!(
                        source = %self.id, first_on,
                        offset_ms = now / 1_000_000,
                        "aligned source timeline onto the programme"
                    );
                    now
                }
            }
        };
        self.vpad.set_offset(offset);
        self.apad.set_offset(offset);
        self.applied.store(true, Ordering::Relaxed);
        offset
    }

    /// The offset in force, or `None` while the next segment is to decide it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn offset(&self) -> Option<i64> {
        *self.offset.lock()
    }

    /// Recompute on the next segment. A restarted source begins its running time
    /// again from zero, so the previous offset no longer holds. A flushing seek
    /// does the same thing for the same reason: see `Mixer::seek`.
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
    /// The mosaic's demand counter. Alive whether or not the pipeline is.
    mv: MultiviewHandle,
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
    /// Consecutive rebuilds of a source that did not bring it back to life,
    /// and the moment before which the next one must not start. Cleared the
    /// moment the source delivers a frame, so a source that recovers is back
    /// on the fast path immediately.
    rebuild_failures: HashMap<SourceId, u32>,
    rebuild_not_before: HashMap<SourceId, Instant>,
    /// The branch of a source that is being rebuilt, kept in the programme
    /// pipeline so its last frame stays on air. See `retire_branch`.
    retired: Vec<RetiredBranch>,
}

/// What is left of a source whose pipeline has been stopped for a rebuild:
/// the programme-side branch, still linked to the compositor, still holding
/// the last frame that came through it.
///
/// A rebuild used to take the whole branch out at once, and `detach_source`
/// takes the programme to None when it removes the source that is on it, so
/// every one of the 1174 rebuilds on 2026-09-12 cut the output to black for
/// the ten or so seconds the browser took to come back. The compositor keeps
/// drawing a pad's last buffer for as long as the pad is there, so leaving the
/// pad in place is a freeze frame that costs nothing: no element to add, no
/// picture to copy, no code path that only runs during a fault.
struct RetiredBranch {
    id: SourceId,
    vpad: gst::Pad,
    apad: gst::Pad,
    branch: Vec<gst::Element>,
    /// Dropped whether or not the rebuild ever finishes. A source that never
    /// comes back must not leave its elements in the programme pipeline.
    until: Instant,
}

/// The numbers `Mixer::timeline_of` gathers. Plain data so that gathering them
/// and writing them out are separate, and so the gathering can be read.
#[derive(Debug, Clone, Copy)]
struct SourceTimeline {
    program_running_ms: u64,
    video_running_ms: Option<u64>,
    audio_running_ms: Option<u64>,
    /// Programme running time minus the buffer's. Positive is behind the
    /// programme, which is ordinary; negative is ahead of it, which is the
    /// fault this was written to catch.
    video_behind_ms: Option<i64>,
    audio_behind_ms: Option<i64>,
    video_buffers: u64,
    audio_buffers: u64,
    vq_buffers: u32,
    vq_time_ms: u64,
    aq_buffers: u32,
    aq_time_ms: u64,
    /// A layered source only: what each side of its compositor has done. See
    /// `LayerCounts`. Zeroes for every other kind of source, which has no
    /// compositor of its own.
    layers: Option<[u64; 5]>,
}

/// How long a frozen frame may stay on air.
///
/// A rebuild of a superimposed source is a page probe (up to
/// `MEDIA_PROBE_TIMEOUT`), a clip fetch and a browser start, and measured on
/// this machine against a local page that came to 22 seconds. Twenty was not
/// enough: the hold ran out two seconds before the source came back and the
/// programme went to the slate for exactly those two seconds, which is the
/// fault this exists to prevent. Forty-five covers the measured rebuild twice
/// over and is still short enough that an operator looking at a still picture
/// is not left wondering for a minute whether the mixer has died.
const FREEZE_HOLD: Duration = Duration::from_secs(45);

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
        // Defensive. The mixer is a live aggregator whose inputs are stamped
        // by another process against its own wall clock, so a contiguous
        // stream into the encoder is worth guaranteeing rather than assuming.
        // Added while chasing an on-air fault that turned out to live in the
        // RTMP server downstream, which re-serves AAC with its timeline
        // stepped backwards; this output, captured straight into ffmpeg,
        // measured clean without it. Kept because it costs nothing and closes
        // a gap that would otherwise be real the day an input drifts.
        let arate = make("audiorate", "aenc-rate")?;
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
                &amix, &amix_caps, &level, &araw_tee, &aenc_q, &aconv, &arate, &aenc, &aparse, &aenc_tee,
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
        gst::Element::link_many([&araw_tee, &aenc_q, &aconv, &arate, &aenc, &aparse, &aenc_tee])
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

        // The mosaic asks for itself through the mixer's own queue, so the
        // pipeline is still created and destroyed on the one thread that owns
        // GStreamer state changes.
        let mv = MultiviewHandle::new(cfg.multiview.clone(), rt.clone(), {
            let h = handle.clone();
            Arc::new(move |d| {
                let _ = h.send(Command::Multiview(d));
            })
        });

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
            mv,
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
            rebuild_failures: HashMap::new(),
            rebuild_not_before: HashMap::new(),
            retired: Vec::new(),
        };
        Ok((mixer, handle, rx, bus_rx))
    }

    /// Persist runtime source and output changes to this path.
    pub fn persist_runtime_to(&mut self, path: std::path::PathBuf) {
        self.runtime_store = Some(path);
    }

    /// Bring up outputs, sources, then start rolling.
    ///
    /// The mosaic is not built here. Nothing runs unless asked: it appears
    /// when the first client subscribes through `MultiviewHandle` and goes
    /// again when the last one leaves. See `multiview.rs`.
    pub fn start(&mut self) -> Result<()> {
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
        // The programme's latency must not depend on the state of a source's
        // own pipeline. See `answer_latency_here`.
        gstutil::answer_latency_here(&vsrc)?;
        gstutil::answer_latency_here(&asrc)?;

        // The operator's desk for this source: a fader, a meter, and a mute, in
        // that order, and the order is the whole point.
        //
        // The fader is ahead of the meter so that pulling it down visibly pulls
        // the meter down with it. A meter that ignored the fader sitting next to
        // it would read as broken, and an operator would stop trusting either.
        //
        // The mute is behind the meter so that a muted source still shows its
        // signal. That is what lets someone confirm a camera has sound on it
        // before cutting to it, which is the whole reason the meter is there.
        //
        // The audiomixer sink pad's own `volume` is left out of this. Takes and
        // transitions fade that pad (see `ramp_volumes`), and two things writing
        // one property fight: whichever wrote last wins, so an operator's fader
        // would be undone by the next take, or the take's fade would be undone
        // mid-ramp by a fader.
        //
        // No `audioconvert` ahead of the fader. The input pipeline ends its audio
        // branch at a capsfilter on the canvas format, so what arrives through
        // the proxy is already raw audio the way the audiomixer wants it, and
        // both `volume` and `level` take that as it is.
        let again = make("volume", &format!("pgm-again-{id}"))?;
        again.set_property("volume", cfg.gain.clamp(0.0, MAX_SOURCE_GAIN));
        let alevel = make("level", &meter_name(id))?;
        crate::probe::set_bool(&alevel, "post-messages", true);
        crate::probe::set_int(&alevel, "interval", 100_000_000);
        let amute = make("volume", &format!("pgm-amute-{id}"))?;
        amute.set_property("mute", cfg.muted);

        // Appended rather than inserted, so the indices the rest of this
        // function uses for the video and audio queues still mean what they did.
        let branch = vec![vsrc, vq, asrc, aq, again.clone(), alevel, amute.clone()];
        self.program.add_many(&branch).context("adding source branch")?;
        gst::Element::link_many([&branch[0], &branch[1]]).context("linking source video")?;
        gst::Element::link_many([&branch[2], &branch[3], &branch[4], &branch[5], &branch[6]])
            .context("linking source audio")?;

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
        // The mute is the last thing before the mixer, so it is what links in.
        branch[6].static_pad("src").unwrap().link(&apad).context("linking audio into mixer")?;

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
            again,
            amute,
            meter: meter_name(&cfg.id),
            stalled_ticks: 0,
            first_reported: false,
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
        let mut cfg = slot.input.config.clone();
        // Carry the desk across. The config this source was built with holds the
        // fader it started at, and a source that comes back an hour later at
        // that value rather than at the one the operator set would jump in level
        // on air, for no reason the operator could see.
        cfg.gain = slot.gain();
        cfg.muted = slot.muted();
        let was_program = self.program_source.as_ref() == Some(id);
        // A frozen frame is only worth freezing if there is one. A source that
        // stalled before its first picture ever arrived has nothing on its
        // compositor pad, so holding its branch on air holds nothing: the pad
        // draws no pixels, the slate under it shows through, and the programme
        // is black for the whole of the rebuild and for every rebuild after
        // that. That is the case this counts, and `handed_over` below is what
        // is done about it.
        let has_picture = slot.input.last_video.seen() > 0;
        self.log_timeline(id, "about to rebuild", true);
        // The source that is on air keeps its last frame on air. Everything
        // else is removed the way the API removes a source: its pipeline
        // stopped first, then its branch taken out of the programme. That order
        // matters. The branch's proxy source shares a stream lock with the
        // thread that pushes this source's frames into the programme; stop the
        // branch while that thread is still pushing and the two wait on each
        // other.
        let held = was_program && self.cfg.stall.hold_last_frame && has_picture;
        let removed = if held { self.retire_branch(id) } else { self.remove_source(id) };
        if let Err(e) = removed {
            warn!(source = %id, ?e, "could not remove the failed source before building it again");
        }
        // Nothing of this source's own to look at, so anything else that is
        // delivering is better than black. After the removal, because until
        // then this source still holds the programme. The rebuilt source comes
        // back on programme when it works (see `finish_rebuild`), so this is a
        // loan and not a decision the operator has to undo.
        let handed_over = if was_program && !has_picture {
            self.hand_programme_to_another_source(id)
        } else {
            None
        };
        // Counted after the removal, which clears everything held under this
        // id, and before the attempt, because nothing on this thread finds out
        // whether a rebuild worked: the tick clears this the moment the source
        // delivers a frame, and if it never does the count stands and the
        // backoff in `arm_source_restart` reads it.
        let failures = self.rebuild_failures.entry(id.clone()).or_insert(0);
        *failures += 1;
        let failures = *failures;
        info!(
            source = %id,
            was_program,
            held,
            has_picture,
            handed_over = ?handed_over,
            failures,
            "building the superimposed source again from scratch"
        );
        self.rebuilding.insert(id.clone(), was_program);
        if let Err(e) = self.begin_add_source(cfg, None) {
            error!(source = %id, ?e, "could not begin building the source again");
            self.rebuilding.remove(id);
        }
    }

    /// Put some other live source on programme, for a source that has none of
    /// its own picture to leave behind.
    ///
    /// Returns which one, or None when there is nothing else delivering and
    /// the slate is the only honest answer. Ordered by the desk, so the choice
    /// is the operator's own ordering and not an accident of when a source was
    /// added; and it must have delivered a picture, because taking a second
    /// source with nothing on its pad would leave the programme exactly as
    /// black as it was.
    fn hand_programme_to_another_source(&mut self, avoid: &SourceId) -> Option<SourceId> {
        let next = self
            .sources
            .iter()
            .find(|s| {
                &s.input.id != avoid
                    && s.input.last_video.seen() > 0
                    && matches!(s.input.observed_state(), SourceState::Live)
            })
            .map(|s| s.input.id.clone())?;
        match self.take(Some(next.clone()), None) {
            Ok(()) => {
                warn!(
                    source = %avoid,
                    now_on_programme = %next,
                    "the source being rebuilt never delivered a picture; the programme is on another source meanwhile"
                );
                let _ = self.events.send(Event::Alert {
                    severity: Severity::Warning,
                    message: format!("{avoid} has no picture to hold; {next} is on programme meanwhile"),
                });
                Some(next)
            }
            Err(e) => {
                warn!(source = %next, ?e, "could not hand the programme to another source");
                None
            }
        }
    }

    /// Stop a source but leave its programme branch where it is, so the
    /// compositor keeps drawing the last frame that came through it.
    ///
    /// Everything `detach_source` does except the two things that take the
    /// picture away: releasing the compositor pad and pulling the elements out
    /// of the programme pipeline. Those happen in `release_retired`, once the
    /// source that replaces it is on air.
    fn retire_branch(&mut self, id: &SourceId) -> Result<()> {
        // Anything already held for this id goes now. Two frozen frames of the
        // same source would be two branches in the pipeline and only the newer
        // one is worth looking at.
        self.release_retired(Some(id));
        let Some(pos) = self.sources.iter().position(|s| &s.input.id == id) else {
            anyhow::bail!("no such source {id}");
        };
        let slot = self.sources.remove(pos);
        if let Some(mv) = &mut self.multiview {
            mv.remove_tile(id).ok();
        }
        slot.input.stop();
        // Under the programme layer and over the slate, so whatever is taken
        // next draws straight over it, and silent: the pipeline behind it has
        // stopped and there is nothing left to hear.
        slot.vpad.set_property("zorder", 1u32);
        slot.vpad.set_property("alpha", 1.0f64);
        slot.apad.set_property("volume", 0.0f64);
        // A meter keeps the name of the source it was built for, and the
        // replacement builds one with the same name. Silenced here so that the
        // two cannot both be read as the new source's level.
        for el in &slot.branch {
            if el.name() == slot.meter.as_str() {
                crate::probe::set_bool(el, "post-messages", false);
            }
        }
        info!(source = %id, "source stopped, its last frame held on the programme");
        self.retired.push(RetiredBranch {
            id: id.clone(),
            vpad: slot.vpad,
            apad: slot.apad,
            branch: slot.branch,
            until: Instant::now() + FREEZE_HOLD,
        });
        self.broadcast_status();
        Ok(())
    }

    /// Let go of held branches: the one for `id`, and any whose hold has run
    /// out. A frozen frame that nothing is coming back to is worse than black,
    /// because it looks like a working picture.
    fn release_retired(&mut self, id: Option<&SourceId>) {
        let now = Instant::now();
        let mut expired = Vec::new();
        let mut i = 0;
        while i < self.retired.len() {
            let r = &self.retired[i];
            if id == Some(&r.id) || r.until <= now {
                let stale = id != Some(&r.id);
                expired.push((self.retired.remove(i), stale));
            } else {
                i += 1;
            }
        }
        for (r, stale) in expired {
            for el in &r.branch {
                let _ = el.set_state(gst::State::Null);
                let _ = self.program.remove(el);
            }
            self.vmix.release_request_pad(&r.vpad);
            self.amix.release_request_pad(&r.apad);
            debug!(source = %r.id, stale, "released the held branch of a rebuilt source");
            // Held as long as it was worth holding and the source never came
            // back. Black is at least honest about that.
            if stale
                && self.program_source.as_ref() == Some(&r.id)
                && !self.sources.iter().any(|s| s.input.id == r.id)
            {
                warn!(source = %r.id, "the held frame has run out and the source has not come back");
                let _ = self.events.send(Event::Alert {
                    severity: Severity::Error,
                    message: format!("{} did not come back; the programme is on the slate", r.id),
                });
                let _ = self.take(None, None);
            }
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
                // After the take, never before it. The held frame is what is on
                // air until the replacement's own pad is drawn, and releasing it
                // first is the black frame this whole arrangement exists to
                // avoid.
                self.release_retired(Some(&cfg.id));
            }
            Err(e) => {
                // Try again, and keep trying: a browser that will not start now
                // may start later, and the source was wanted. The delay grows
                // with the failures, which is what stops this being the twelve
                // second loop that ran for two hours on 2026-09-12.
                let failures = *self.rebuild_failures.get(&cfg.id).unwrap_or(&0);
                let delay = self.cfg.stall.rebuild_delay(failures).unwrap_or(Duration::from_secs(10));
                warn!(source = %cfg.id, ?e, failures, ?delay, "building the source again failed; trying again later");
                self.rebuilding.insert(cfg.id.clone(), was_program);
                let handle = self.handle.clone();
                let again = Box::new(cfg.clone());
                self.rt.spawn(async move {
                    tokio::time::sleep(delay).await;
                    let _ = handle.send(Command::AddSource(again, None));
                });
            }
        }
    }

    /// Where a source's last picture and last sound had got to, against the
    /// programme's own clock and the queues that carry them across.
    ///
    /// The one set of numbers that would have named the stall of 2026-09-11 and
    /// 2026-09-12, where a superimposed source came up with every layer placed
    /// normally and then never delivered a picture while its sound flowed. Its
    /// two aggregators are force-live and emit black and silence on schedule
    /// with every layer dead, so a source producing nothing was not starved: it
    /// was blocked downstream, and the only thing downstream that can block it
    /// is this pipeline's compositor holding buffers it is not ready to consume.
    /// A compositor holds what is timed ahead of where it has got to, its pad
    /// queue then fills, and the push into it never returns.
    ///
    /// So `behind_ms` is the number to read. Positive means the buffer was
    /// behind the programme, which is ordinary and is what a healthy source
    /// shows. Negative means it was in the programme's future, which is the
    /// fault, and `vq_buffers` climbing with it is the block itself.
    fn timeline_of(&self, slot: &SourceSlot) -> SourceTimeline {
        let now = self.running_time().unwrap_or(gst::ClockTime::ZERO);
        let behind = |at: Option<gst::ClockTime>| {
            at.map(|at| now.nseconds() as i64 / 1_000_000 - at.nseconds() as i64 / 1_000_000)
        };
        let video = slot.input.last_video.running();
        let audio = slot.input.last_audio.running();
        // The programme-side queues for this source: `pgm-vq-<id>` and
        // `pgm-aq-<id>`, which are the branch's second and fourth elements.
        let level = |q: Option<&gst::Element>| {
            q.map(|q| (q.property::<u32>("current-level-buffers"), q.property::<u64>("current-level-time") / 1_000_000))
                .unwrap_or((0, 0))
        };
        let (vq_buffers, vq_time_ms) = level(slot.branch.get(1));
        let (aq_buffers, aq_time_ms) = level(slot.branch.get(3));
        SourceTimeline {
            program_running_ms: now.mseconds(),
            video_running_ms: video.map(|t| t.mseconds()),
            audio_running_ms: audio.map(|t| t.mseconds()),
            video_behind_ms: behind(video),
            audio_behind_ms: behind(audio),
            video_buffers: slot.input.last_video.seen(),
            audio_buffers: slot.input.last_audio.seen(),
            vq_buffers,
            vq_time_ms,
            aq_buffers,
            aq_time_ms,
            layers: slot.input.layer_counts().map(|c| c.read()),
        }
    }

    /// Write one of those out. Called when a source is judged stalled, again
    /// just before it is rebuilt, and once when its first picture arrives, so
    /// a build that worked and a build that did not can be read side by side.
    fn log_timeline(&self, id: &SourceId, why: &'static str, stalled: bool) {
        let Some(slot) = self.sources.iter().find(|s| &s.input.id == id) else { return };
        let t = self.timeline_of(slot);
        if stalled {
            warn!(
                source = %id, why,
                program_running_ms = t.program_running_ms,
                video_running_ms = ?t.video_running_ms,
                video_behind_ms = ?t.video_behind_ms,
                audio_running_ms = ?t.audio_running_ms,
                audio_behind_ms = ?t.audio_behind_ms,
                video_buffers = t.video_buffers,
                audio_buffers = t.audio_buffers,
                vq_buffers = t.vq_buffers,
                vq_time_ms = t.vq_time_ms,
                aq_buffers = t.aq_buffers,
                aq_time_ms = t.aq_time_ms,
                page_in = ?t.layers.map(|l| l[0]),
                media_in = ?t.layers.map(|l| l[1]),
                comp_out = ?t.layers.map(|l| l[2]),
                rate_out = ?t.layers.map(|l| l[3]),
                mix_out = ?t.layers.map(|l| l[4]),
                "where this source's last buffers sat on the programme's timeline"
            );
        } else {
            info!(
                source = %id, why,
                program_running_ms = t.program_running_ms,
                video_running_ms = ?t.video_running_ms,
                video_behind_ms = ?t.video_behind_ms,
                audio_running_ms = ?t.audio_running_ms,
                audio_behind_ms = ?t.audio_behind_ms,
                video_buffers = t.video_buffers,
                audio_buffers = t.audio_buffers,
                vq_buffers = t.vq_buffers,
                vq_time_ms = t.vq_time_ms,
                aq_buffers = t.aq_buffers,
                aq_time_ms = t.aq_time_ms,
                page_in = ?t.layers.map(|l| l[0]),
                media_in = ?t.layers.map(|l| l[1]),
                comp_out = ?t.layers.map(|l| l[2]),
                rate_out = ?t.layers.map(|l| l[3]),
                mix_out = ?t.layers.map(|l| l[4]),
                "where this source's first buffers sat on the programme's timeline"
            );
        }
    }

    /// Take a source out of the desk without stopping its pipeline. What
    /// remains is the caller's, to stop here or elsewhere.
    fn detach_source(&mut self, id: &SourceId) -> Result<SourceSlot> {
        let Some(pos) = self.sources.iter().position(|s| &s.input.id == id) else {
            anyhow::bail!("no such source {id}");
        };
        // A held frame of this same source goes with it. Somebody removing a
        // source wants it gone, not a still of it left on the compositor.
        self.release_retired(Some(id));
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
        // Everything this module keeps under the source's id goes with it. The
        // ids are reused: a director alternates two of them, one per match, so
        // a restart delay or a rebuild count left behind from the last source
        // called event-a would be charged to the next one, which is a
        // different page in a different state.
        self.source_attempts.remove(id);
        self.rebuild_failures.remove(id);
        self.rebuild_not_before.remove(id);
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

    /// The sources and outputs as configured, live ones and those still being
    /// probed. This is the list the runtime store is written from, and what
    /// the API reads when it needs real URLs: `status()` masks them, because
    /// an RTMP address carries the stream key.
    fn runtime_configs(&self) -> RuntimeConfigs {
        let mut sources: Vec<SourceConfig> = self
            .sources
            .iter()
            .filter(|s| s.input.id != AD_ID)
            .map(|s| {
                let mut cfg = s.input.config.clone();
                // Taken off the elements rather than from the config the source
                // was built with. The config is the value it started at; these
                // are where the operator left the desk, and that is what has to
                // come back after a restart.
                cfg.gain = s.gain();
                cfg.muted = s.muted();
                cfg
            })
            .collect();
        for p in &self.pending {
            if !sources.iter().any(|c| c.id == p.id) {
                sources.push(p.clone());
            }
        }
        let outputs: Vec<OutputConfig> = self.outputs.iter().map(|o| o.cfg.clone()).collect();
        RuntimeConfigs { sources, outputs }
    }

    /// Write the current source list beside the config file.
    ///
    /// Sources added or removed from the UI have to survive a restart, and
    /// rewriting the operator's own config file would throw away its comments
    /// and layout. Once this sidecar exists it is the authoritative list, which
    /// keeps "where do sources come from" a question with a single answer.
    fn persist_runtime(&self) {
        let Some(path) = &self.runtime_store else { return };
        let RuntimeConfigs { sources: live, outputs } = self.runtime_configs();

        #[derive(serde::Serialize)]
        struct Stored<'a> {
            sources: &'a [SourceConfig],
            outputs: &'a [OutputConfig],
        }
        let body = match toml::to_string_pretty(&Stored { sources: &live, outputs: &outputs }) {
            Ok(b) => format!(
                "# Sources and outputs managed from the GodwinMix UI or API.\n\
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
            // An ad goes out at the level the file was made at. There is no
            // operator fader for it: it is not in the source list, so nothing
            // draws one, and a break that went out silent because the last
            // source's fader happened to be down would be worse than useless.
            gain: crate::state::unity_gain(),
            muted: false,
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
            Command::Multiview(d) => self.multiview_demand(d)?,
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
            Command::SetAudio { source, gain, muted, page, media, reply } => {
                let _ = reply.send(self.set_audio(&source, gain, muted, page, &media));
            }
            Command::Seek { source, position_ms, reply } => {
                let _ = reply.send(self.seek(&source, position_ms));
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
            Command::Configs(reply) => {
                let _ = reply.send(self.runtime_configs());
            }
            Command::Bus(ev) => self.on_bus(ev),
            Command::Tick => self.tick(),
            Command::PositionTick => self.position_tick(),
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
                        self.arm_source_restart(id, "the pipeline posted an error");
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
            BusEvent::Level { src, peak_db } => {
                // Every meter in the programme posts on this one bus, so the
                // element's name is what says whose peaks these are. Matched
                // whole: a source id may contain a hyphen, so splitting the name
                // would attribute `pgm-alevel-cam-1` to a source called `cam`.
                if src == "pgm-level" {
                    let _ = self.events.send(Event::AudioLevel { peak_db });
                } else if let Some(slot) = self.sources.iter().find(|s| s.meter == src) {
                    let _ = self.events.send(Event::SourceAudioLevel {
                        source: slot.input.id.clone(),
                        peak_db,
                    });
                }
                // Anything else is a meter nothing is listening for, or one
                // belonging to a source that has just been removed. Dropped
                // silently: these arrive ten times a second and a log line per
                // message would bury everything else.
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
                    // A source that ends is also worth a timeline line: the
                    // question is the same one, where its last buffers sat.
                    let id = id.to_string();
                    self.log_timeline(&id, "end of stream", true);
                    self.arm_source_restart(id, "it reached the end of its stream");
                }
            }
        }
    }

    fn tick(&mut self) {
        // Reassert visibility so a stall fades to slate and a recovery fades
        // back, without either needing its own event.
        self.apply_visibility(true);


        // Held frames that have run out of time. Before the liveness sweep, so
        // a source that has come back releases its own held frame there rather
        // than here.
        self.release_retired(None);

        let mut restart = Vec::new();
        // Sources whose first picture has just arrived, and sources that have
        // just been judged stalled. Collected here and written out below,
        // because the report reads the whole slot and this loop holds it.
        let mut first_picture = Vec::new();
        let mut judged_stalled = Vec::new();
        let fallback_ticks = (CLIENT_FALLBACK_AFTER.as_millis() / TICK.as_millis()) as u32;
        let stall_ticks = ((self.cfg.stall.restart_after_secs * 1000).max(TICK.as_millis() as u64)
            / TICK.as_millis() as u64) as u32;
        for slot in &mut self.sources {
            // Whether this source can be scrubbed is asked here rather than
            // where it is reported. Nothing answers a SEEKING query until the
            // chain from the source element to the proxies is built, which for a
            // file took between 50 ms and a second on this machine, so the
            // question has to be asked again until it is answered. Once answered
            // it is kept, and this costs nothing thereafter.
            slot.input.refresh_seekable();

            if !slot.first_reported && slot.input.last_video.seen() > 0 {
                slot.first_reported = true;
                first_picture.push(slot.input.id.clone());
            }

            match slot.input.observed_state() {
                SourceState::Stalled => {
                    slot.stalled_ticks += 1;
                    if slot.stalled_ticks == 1 {
                        judged_stalled.push(slot.input.id.clone());
                    }
                    // Asked on every tick past the mark rather than only on the
                    // tick that reaches it. `arm_source_restart` is the one gate
                    // (a source may have only one restart armed, and a rebuild
                    // may be waiting out its backoff), and a single shot here
                    // meant a refusal there was never asked again.
                    if slot.stalled_ticks >= stall_ticks {
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
        for id in first_picture {
            self.log_timeline(&id, "first picture", false);
        }
        for id in judged_stalled {
            self.log_timeline(&id, "judged stalled", true);
        }
        for id in restart {
            self.arm_source_restart(id, "it has delivered nothing for too long");
        }

        for slot in &self.sources {
            if matches!(slot.input.observed_state(), SourceState::Live) {
                self.source_attempts.insert(slot.input.id.clone(), 0);
                // A source that is delivering has been rebuilt successfully,
                // however many attempts it took, so the backoff starts again
                // from nothing the next time it goes wrong.
                self.rebuild_failures.remove(&slot.input.id);
                self.rebuild_not_before.remove(&slot.input.id);
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
    fn arm_source_restart(&mut self, id: SourceId, why: &'static str) {
        let Some(slot) = self.sources.iter().find(|s| s.input.id == id) else {
            return;
        };
        // A superimposed source is not restarted in place, it is built again
        // from nothing (see `rebuild_source`), which costs a browser launch, a
        // profile directory and about ten seconds of the operator's attention.
        // Doing that every twelve seconds for two hours is what happened on
        // 2026-09-11 and 2026-09-12, so it gets a policy of its own.
        if slot.input.superimposed() {
            let now = Instant::now();
            if self.rebuild_not_before.get(&id).is_some_and(|t| *t > now) {
                return;
            }
            let failures = *self.rebuild_failures.get(&id).unwrap_or(&0);
            if let Some(wait) = self.cfg.stall.rebuild_delay(failures) {
                self.rebuild_not_before.insert(id.clone(), now + wait);
                warn!(source = %id, failures, ?wait, "source has failed to rebuild; waiting before the next attempt");
                let _ = self.events.send(Event::Alert {
                    severity: Severity::Error,
                    message: format!(
                        "{id} has failed {failures} rebuilds in a row; the next is in {} s",
                        wait.as_secs()
                    ),
                });
                // Scheduled rather than dropped, so the retry happens even
                // once the source stops being reported as stalled.
                let handle = self.handle.clone();
                let again = id.clone();
                self.rt.spawn(async move {
                    tokio::time::sleep(wait).await;
                    let _ = handle.send(Command::RestartSource(again));
                });
                return;
            }
        }
        if !slot.input.try_arm_restart() {
            return;
        }
        // `why` rather than a fixed message: this is reached from the stall
        // sweep, from a pipeline error and from an end of stream, and a line
        // that said "stalled" for all three sent the reader looking at the
        // wrong thing on 2026-09-12, when a killed browser arrived here as an
        // end of stream.
        warn!(source = %id, why, "restarting the source's pipeline");
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

    /// Move a source's fader, its mute, or a superimposed source's balance.
    ///
    /// Runs on the mixer thread like every other command, which is what keeps
    /// it away from the streaming threads: setting a `volume` property is
    /// cheap but it is still a pipeline touch, and the control plane has no
    /// business doing those from a request handler.
    fn set_audio(
        &mut self,
        id: &SourceId,
        gain: Option<f64>,
        muted: Option<bool>,
        page: Option<f64>,
        media: &[Option<f64>],
    ) -> AudioOutcome {
        let Some(slot) = self.sources.iter().find(|s| &s.input.id == id) else {
            return AudioOutcome::NoSuchSource;
        };
        let levels = slot.input.levels();
        // Refused before anything moves. A request carrying both a fader and a
        // balance either lands whole or lands not at all, so a caller reading
        // the 409 does not have to wonder which half of it took.
        if levels.is_none() && needs_superimposed(page, media) {
            return AudioOutcome::NotSuperimposed;
        }

        if let Some(gain) = gain {
            slot.set_gain(gain);
        }
        if let Some(muted) = muted {
            slot.set_muted(muted);
        }
        if let Some(levels) = levels {
            if media.len() > levels.media_count() {
                // Not refused: a page can drop a video between the UI drawing
                // its faders and the operator moving one, and losing the gains
                // that did land would be worse than ignoring the ones that
                // cannot.
                warn!(
                    source = %id,
                    asked = media.len(),
                    have = levels.media_count(),
                    "more media gains than this source has videos, the extra ones do nothing"
                );
            }
            levels.apply(page, media);
        }

        let now = slot.audio_state();
        info!(
            source = %id, gain = now.gain, muted = now.muted,
            page = ?now.page, media = ?now.media,
            "source audio changed"
        );
        // The desk has to come back where the operator left it, and the fader
        // and the mute are read off the elements when the source list is
        // written, so saving the list now is all it takes. Only when one of
        // those two was actually named: the balance is not saved anywhere, and
        // an empty body is a read rather than a change, so rewriting the file
        // for either would be a disk write per poll.
        if gain.is_some() || muted.is_some() {
            self.persist_runtime();
        }
        // Every connected UI shares one set of faders, so a change made in one
        // browser has to reach the others. The levels ride along in the
        // source rows of an ordinary status snapshot.
        self.broadcast_status();
        AudioOutcome::Set(now)
    }

    /// Move a source to a position and answer with where it landed.
    ///
    /// # Why the aligner is reset first
    ///
    /// `TimelineAligner` places a source on the programme's timeline by taking
    /// the programme's running time when the source's first segment arrives and
    /// holding that offset for the life of the source. A flushing seek restarts
    /// the segment, so the offset taken from the old one no longer describes
    /// anything: measured on this machine against a thirty second clip, a source
    /// seeked four seconds after it was added came back four seconds behind the
    /// programme, which the mixers consume as fast as it arrives. The clip
    /// fast-forwarded through those four seconds and landed four seconds past the
    /// mark the operator asked for, and the audiomixer threw away every sample it
    /// was handed on the way there, because samples whose running time is in the
    /// past are exactly what it discards. Video looked fine throughout, which is
    /// what makes this worth a paragraph. The longer the source has been up the
    /// worse it gets: a clip added ten minutes ago would race ten minutes of
    /// media through the decoder, or simply run off the end of the file.
    ///
    /// Resetting makes the segment the seek produces establish a fresh offset the
    /// same way the first one did, which puts the source back at the programme's
    /// current running time with its sound. It has to happen before the seek, not
    /// after: the seek flushes, the new segment follows immediately, and on the
    /// measured run it reached the aligner's probe a millisecond after
    /// `seek_simple` returned. Reset it afterwards and the probe has already
    /// reused the stale offset with no further segment coming to correct it.
    ///
    /// Seeking a source that is live on program is safe. The flush travels across
    /// the proxy into this pipeline, which is what empties the queues holding the
    /// old position, and it stops at the compositor and the audiomixer: the slate
    /// and silence pads are never flushed, so neither aggregator forwards the
    /// flush downstream and the encoder, the muxer and the RTMP connection never
    /// see it. Verified by watching both mixers' source pads across a seek. What
    /// the viewer gets is a few hundred milliseconds of the previous frame and no
    /// sound, the same gap the picture has, and then the new position.
    fn seek(&mut self, id: &SourceId, position_ms: u64) -> SeekOutcome {
        let Some(slot) = self.sources.iter().find(|s| &s.input.id == id) else {
            return SeekOutcome::NoSuchSource;
        };
        if !slot.seekable() {
            return SeekOutcome::NotSeekable;
        }
        if let Some(aligner) = &slot.aligner {
            aligner.reset();
        }
        let landed = match slot.input.seek_ms(position_ms) {
            Ok(landed) => landed,
            Err(e) => {
                warn!(source = %id, asked_ms = position_ms, ?e, "seek refused");
                return SeekOutcome::Failed(format!("{e:#}"));
            }
        };
        let now = SourcePositionState { position_ms: landed, duration_ms: slot.input.duration_ms() };
        info!(
            source = %id, asked_ms = position_ms, landed_ms = landed,
            duration_ms = ?now.duration_ms, "source moved"
        );
        // No `persist_runtime` here. A position is not a setting: a restart that
        // resumed a clip half way through would be a surprise nothing in the UI
        // asked for.
        //
        // The snapshot goes out so that every other browser's scrubber jumps with
        // this one rather than waiting for its own poll.
        self.broadcast_status();
        SeekOutcome::Moved(now)
    }

    /// Tell every connected UI where each seekable source has got to.
    ///
    /// Only the seekable ones. A camera has no position to report, and on a nine
    /// camera rig an event per source per tick would be most of what the
    /// WebSocket carried.
    fn position_tick(&self) {
        for slot in &self.sources {
            let Some(at) = slot.position() else { continue };
            let _ = self.events.send(Event::SourcePosition {
                source: slot.input.id.clone(),
                position_ms: at.position_ms,
                duration_ms: at.duration_ms,
            });
        }
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
            .map(|s| {
                // Asked once and used twice: each of these is a query on the
                // source's own pipeline, and the snapshot is polled.
                let at = s.position();
                SourceStatus {
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
                    // Only a superimposed source has levels, so this is `None`
                    // for everything else and the UI draws no faders for it.
                    audio: s.input.levels().map(|l| l.report()),
                    // These two come off the elements, not off the config, so the
                    // snapshot says what the pipeline is doing even after a gain
                    // was clamped on its way in.
                    gain: s.gain(),
                    muted: s.muted(),
                    // Asked of the pipeline, not worked out from the URI, and
                    // `None` for both numbers on anything that cannot be
                    // scrubbed.
                    seekable: s.seekable(),
                    position_ms: at.as_ref().map(|at| at.position_ms),
                    duration_ms: at.and_then(|at| at.duration_ms),
                }
            })
            .collect();

        MixerStatus {
            program: self.program_source.clone(),
            sources,
            outputs: self.outputs.iter().map(|o| o.status()).collect(),
            // With no mosaic running the configured shape is still what a
            // client would get if it asked, so `enabled` answers "may I have
            // one", not "is one running". The cells are empty because there
            // are none until it is built.
            multiview: multiview.unwrap_or_else(|| MultiviewStatus {
                enabled: self.cfg.multiview.enabled,
                width: self.cfg.multiview.width,
                height: self.cfg.multiview.height,
                cols: 0,
                rows: 0,
                cells: Vec::new(),
                fps: self.cfg.multiview.fps,
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

    /// The mosaic's demand counter. Clients subscribe through this; the
    /// pipeline exists only while at least one of them does.
    pub fn multiview_handle(&self) -> MultiviewHandle {
        self.mv.clone()
    }

    /// The programme pipeline itself, so `gmx bench` can put a pad probe on
    /// the encoder and time the first frame out of it. Nothing in the running
    /// mixer uses this.
    pub fn program_pipeline(&self) -> &gst::Pipeline {
        &self.program
    }

    /// Build or destroy the mosaic because the subscriber count changed.
    /// Everything that decides *whether* lives in `multiview.rs`; this is only
    /// the part that has to happen on the mixer thread.
    fn multiview_demand(&mut self, d: Demand) -> Result<()> {
        match d {
            Demand::Build(shape) => {
                if self.multiview.as_ref().is_some_and(|mv| mv.shape() == shape) {
                    return Ok(());
                }
                // A rebuild at another size drops the old one first, so there
                // is never a moment with two mosaic encoders running.
                self.multiview = None;
                self.mv.mark_built(None);
                let mut mv = Multiview::build(&self.mv, shape, &self.pgm_video_proxy)
                    .context("building multiview")?;
                mv.attach_watch(gstutil::watch_bus(
                    mv.pipeline(),
                    "multiview",
                    self.bus_tx.clone(),
                )?);
                for slot in &self.sources {
                    if slot.input.id == AD_ID {
                        continue;
                    }
                    mv.add_tile(Some(slot.input.id.clone()), &slot.input.thumb_proxy)
                        .context("adding multiview tile")?;
                }
                mv.follow_clock_of(&self.program);
                mv.start().context("starting multiview")?;
                self.multiview = Some(mv);
                self.mv.mark_built(Some(shape));
                info!(?shape, "multiview built for a subscriber");
            }
            Demand::Teardown => {
                self.multiview = None;
                self.mv.mark_built(None);
            }
        }
        Ok(())
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
        self.multiview = None;
        self.mv.mark_built(None);
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

    // A second timer rather than more work on the first. The supervisor's tick
    // restarts stalled sources and watches output queues, and none of that wants
    // doing twice as often just because a scrubber needs 4 Hz to look smooth.
    let positions = handle.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(POSITION_TICK);
        loop {
            interval.tick().await;
            if positions.send(Command::PositionTick).is_err() {
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

    /// A level message carries only the name of the element that posted it, and
    /// ids may contain hyphens, so the name has to be matched whole. Splitting
    /// `pgm-alevel-cam-1` on its hyphens names `cam`, and `cam` is a source that
    /// may well exist, so the peaks from one camera would be drawn on another's
    /// meter with nothing anywhere to say it had happened.
    #[test]
    fn a_meter_is_attributed_by_whole_name_not_by_splitting_it() {
        let ids = ["cam", "cam-1", "cam-1-backup", "feed-a-b-c"];
        for id in ids {
            let posted = meter_name(id);
            let matched: Vec<&str> =
                ids.iter().copied().filter(|candidate| meter_name(candidate) == posted).collect();
            assert_eq!(matched, vec![id], "{posted} was attributed to {matched:?}");
        }

        // The mistake this guards against, spelled out.
        let posted = meter_name("cam-1-backup");
        let naive = posted.strip_prefix("pgm-alevel-").unwrap().split('-').next().unwrap();
        assert_eq!(naive, "cam", "the naive read really does go wrong");
        assert_ne!(naive, "cam-1-backup");

        // And the programme's own meter can never be read as a source's, which
        // is what keeps the program bar and the per-source bars apart.
        assert!(ids.iter().all(|id| meter_name(id) != "pgm-level"));
    }

    /// The fader and the mute are elements this mixer owns, one pair per source,
    /// so they work on a camera and a file. Only a balance needs a superimposed
    /// source. Getting this wrong answers 409 to an operator pulling a camera
    /// down, which is the one thing the fader exists for.
    #[test]
    fn a_fader_or_a_mute_does_not_need_a_superimposed_source() {
        // A fader alone, a mute alone, and an empty read all go through.
        assert!(!needs_superimposed(None, &[]));
        // A balance does not.
        assert!(needs_superimposed(Some(0.5), &[]));
        assert!(needs_superimposed(None, &[Some(0.5)]));
        assert!(needs_superimposed(Some(0.5), &[Some(0.5)]));
        // A media list of nothing but nulls names no channel, so it asks for
        // nothing a camera cannot give. The UI sends these: `[null, 0.5]` is how
        // it names the second video without moving the first.
        assert!(!needs_superimposed(None, &[None, None]));
        assert!(needs_superimposed(None, &[None, Some(0.5)]));
    }

    /// A seek has to make the aligner work its offset out again.
    ///
    /// The offset is taken from the programme's running time when a segment
    /// arrives and then held, which is right for a source that plays straight
    /// through. A flushing seek restarts the segment, so the held offset places
    /// the source where it was when it was added rather than where the programme
    /// is now. Reuse it and the source comes back behind the programme by however
    /// long it has been up, the mixers consume that media as fast as it arrives,
    /// and the audiomixer discards every sample of it because samples in the past
    /// are exactly what it drops. The picture looks fine the whole time.
    ///
    /// So this proves the thing that matters: after `reset`, the next segment
    /// computes a fresh offset, and both pads get it.
    #[test]
    fn a_seek_makes_the_aligner_work_the_offset_out_again() {
        let _ = gst::init();
        let vpad = gst::Pad::builder(gst::PadDirection::Sink).name("vsink").build();
        let apad = gst::Pad::builder(gst::PadDirection::Sink).name("asink").build();
        let aligner = TimelineAligner {
            id: "clip1".into(),
            offset: Mutex::new(None),
            applied: AtomicBool::new(false),
            vpad: vpad.clone(),
            apad: apad.clone(),
        };
        assert_eq!(aligner.offset(), None, "nothing is placed before a segment arrives");

        // The source is added a minute into the programme. Its first segment
        // decides the offset.
        let minute = 60_000_000_000i64;
        let first = aligner.place_at(minute, "audio");
        assert_eq!(first, minute);
        assert_eq!(aligner.offset(), Some(minute));
        // Both pads, from whichever branch arrived first, which is what keeps
        // the source in lip sync.
        assert_eq!(vpad.offset(), minute);
        assert_eq!(apad.offset(), minute);

        // The other branch's segment arrives a few milliseconds later and must
        // reuse the offset rather than take its own.
        assert_eq!(aligner.place_at(minute + 8_000_000, "video"), first);
        assert_eq!(vpad.offset(), first);

        // Now the seek, fifteen seconds further into the programme. Without a
        // reset the stale offset is reused, and that is the bug: the source would
        // be placed a minute in while the programme is at 1:15.
        let after_seek = minute + 15_000_000_000;
        assert_eq!(
            aligner.place_at(after_seek, "audio"),
            first,
            "with no reset the old offset is reused, which is what breaks the sound"
        );

        // With the reset it works the offset out again from where the programme
        // actually is.
        aligner.reset();
        assert_eq!(aligner.offset(), None);
        let second = aligner.place_at(after_seek, "audio");
        assert_eq!(second, after_seek);
        assert_ne!(second, first, "the offset has to be recomputed, not reused");
        assert_eq!(vpad.offset(), second);
        assert_eq!(apad.offset(), second);
        // And the branch that follows shares the new one, so the seek does not
        // cost lip sync either.
        assert_eq!(aligner.place_at(after_seek + 12_000_000, "video"), second);
        assert_eq!(apad.offset(), second);
    }

    #[test]
    fn building_outside_a_runtime_fails_with_a_clear_message() {
        let _ = gst::init();
        let err = Mixer::build(crate::config::Config {
            canvas: Default::default(),
            program: Default::default(),
            multiview: Default::default(),
            snapshot: Default::default(),
            control: Default::default(),
            hardware: Default::default(),
            media: Default::default(),
            security: Default::default(),
            browser: Default::default(),
            stall: Default::default(),
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
