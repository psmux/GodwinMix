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
use crate::input::{InputPipeline, MediaReport};
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

/// How many commands may be waiting for the mixer thread.
///
/// The queue was unbounded, which is a memory leak wearing a helpful face: a
/// client in a loop, or a plugin posting bus messages faster than they can be
/// read, grows it without limit and the work still only happens at the rate
/// one thread can do it. Bounded, a caller is told to come back rather than
/// being quietly enqueued behind a thousand others. Deep enough that a burst
/// from a UI redraw or a scene apply never touches it; shallow enough that
/// what is in it can still be worked through in well under a second.
pub const COMMAND_QUEUE: usize = 256;

/// How many bus messages may be waiting. Larger than the command queue
/// because several pipelines post onto it and a `level` element alone posts
/// ten a second per source.
pub const BUS_QUEUE: usize = 512;

/// What a refused caller is told to wait. Two ticks of the supervisor, which
/// is long enough for a full queue to have drained on any machine that is not
/// already in trouble.
const BUSY_RETRY_MS: u64 = 1000;

/// What to tell a caller when the mixer's queue is full.
///
/// Its own type so the control plane can answer -32001 with `retry_after_ms`
/// rather than turning a full queue into a generic failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Busy {
    /// How long to wait before trying again, in milliseconds.
    pub retry_after_ms: u64,
    /// How deep the queue is, so the message can say what was full.
    pub queue: usize,
}

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the mixer already has {} commands waiting and will not take another. \
             Nothing was changed. Try again in {} ms",
            self.queue, self.retry_after_ms
        )
    }
}

impl std::error::Error for Busy {}

/// One tick's worth of the mixer's own housekeeping, so two timers that fire
/// while the mixer is busy do not both queue.
///
/// A supervisor tick is worth doing once, not once per timer fire. Without
/// this, a mixer held up by a slow state change came back to a queue of
/// identical ticks and did the same work over and over before reaching the
/// command an operator was waiting on.
#[derive(Debug, Default)]
struct Coalesced {
    tick: AtomicBool,
    position: AtomicBool,
}

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
use crate::plugin::branch::{BranchCtx, ProgrammeBranch};

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
    /// Put a filter on a source or on the programme, while live. The insert is
    /// under a pad block, so the programme loses at most the one frame the
    /// block holds.
    AddFilter(Box<crate::config::FilterConfig>, Option<Ack>),
    /// Change a filter that is already in place. A filter that cannot take the
    /// change while running says so and nothing is torn down.
    SetFilter {
        id: String,
        params: crate::config::Params,
        reply: oneshot::Sender<FilterOutcome>,
    },
    /// Take a filter out, relinking around it under the same pad block.
    RemoveFilter(String, Option<Ack>),
    /// Every filter in place, with where it sits.
    ListFilters(oneshot::Sender<Vec<FilterStatus>>),
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

/// Where one filter sits and what it is, for a listing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilterStatus {
    pub id: String,
    #[serde(rename = "type")]
    pub type_id: String,
    /// The source it is attached to, or None for a programme filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceId>,
    pub side: String,
}

/// What `filter.set` answers with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterOutcome {
    /// The change took effect on the running filter.
    Applied,
    /// The filter cannot take the change while running, and says why.
    RestartRequired(String),
    NoSuchFilter(String),
    Failed(String),
}

#[derive(Clone)]
pub struct MixerHandle {
    tx: mpsc::Sender<Command>,
    events: EventBus,
    coalesced: Arc<Coalesced>,
}

impl MixerHandle {
    /// Queue a command. Never blocks: this is called from GStreamer clock
    /// callbacks and from the bus thread as well as from the control plane,
    /// and none of those may wait on the mixer thread.
    pub fn send(&self, cmd: Command) -> Result<()> {
        match self.tx.try_send(cmd) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(anyhow::anyhow!("mixer is not running"))
            }
            Err(mpsc::error::TrySendError::Full(cmd)) => {
                let label = Mixer::label(&cmd);
                warn!(command = label, "the mixer queue is full; the command was refused");
                Err(Busy { retry_after_ms: BUSY_RETRY_MS, queue: COMMAND_QUEUE }.into())
            }
        }
    }

    /// Ask for a supervisor tick, unless one is already waiting.
    fn tick(&self) -> Result<()> {
        if self.coalesced.tick.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.send(Command::Tick).inspect_err(|_| {
            self.coalesced.tick.store(false, Ordering::SeqCst);
        })
    }

    /// Ask for a position report, unless one is already waiting.
    fn position_tick(&self) -> Result<()> {
        if self.coalesced.position.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.send(Command::PositionTick).inspect_err(|_| {
            self.coalesced.position.store(false, Ordering::SeqCst);
        })
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

    /// Put a filter on a source or on the programme, live.
    ///
    /// The public shape the API agent should wire `filter.add` to. It answers
    /// once the filter is actually in the pipeline, so a caller that gets `Ok`
    /// knows the picture has changed.
    pub async fn add_filter(&self, cfg: crate::config::FilterConfig) -> Result<()> {
        self.request(|ack| Command::AddFilter(Box::new(cfg), Some(ack))).await
    }

    /// Change a filter in place. `filter.set`.
    pub async fn set_filter(
        &self,
        id: String,
        params: crate::config::Params,
    ) -> Result<FilterOutcome> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::SetFilter { id, params, reply: tx })?;
        rx.await.map_err(|_| anyhow::anyhow!("mixer dropped the filter request"))
    }

    /// Take a filter out. `filter.remove`.
    pub async fn remove_filter(&self, id: String) -> Result<()> {
        self.request(|ack| Command::RemoveFilter(id, Some(ack))).await
    }

    /// Every filter in place. `filter.list`.
    pub async fn filters(&self) -> Result<Vec<FilterStatus>> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::ListFilters(tx))?;
        rx.await.map_err(|_| anyhow::anyhow!("mixer dropped the filter listing"))
    }

    pub async fn configs(&self) -> Result<RuntimeConfigs> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Configs(tx))?;
        rx.await.map_err(|_| anyhow::anyhow!("mixer dropped the configs request"))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Envelope> {
        self.events.subscribe()
    }

    /// The sequence number of the last event published, so a caller taking a
    /// status snapshot can say which point in the stream it is current as of.
    pub fn event_seq(&self) -> u64 {
        self.events.seq()
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
    /// This source's side of the proxy boundary: the proxysrcs, the queues,
    /// the fader, the meter, the mute and the two mixer pads. See
    /// `ProgrammeBranch`.
    branch: ProgrammeBranch,
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

impl SourceSlot {
    fn gain(&self) -> f64 {
        self.branch.gain()
    }

    fn set_gain(&self, gain: f64) {
        self.branch.set_gain(gain)
    }

    fn muted(&self) -> bool {
        self.branch.muted()
    }

    fn set_muted(&self, muted: bool) {
        self.branch.set_muted(muted)
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
    events: EventBus,
    /// The mixer runs on a plain OS thread, because GStreamer state changes
    /// block and must never sit on a Tokio worker. That thread has no runtime
    /// context of its own, so `tokio::spawn` from it panics. Delayed work
    /// (reconnect backoff, source restarts) goes through this handle instead.
    rt: tokio::runtime::Handle,
    /// Every pipeline we create gets a bus watcher feeding this. Held here so
    /// that a source added mid-broadcast is supervised exactly like one from
    /// the config file.
    bus_tx: mpsc::Sender<BusEvent>,
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
    /// Filters living in the programme pipeline: on the programme itself, and
    /// on a source's programme side branch. A filter on a source's input side
    /// lives in that source's own pipeline instead.
    programme_filters: Vec<crate::plugin::FilterSlot>,
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


// ---------------------------------------------------------------------------
// Building the programme chain from catalogue entries
//
// None of these name a codec, a parser or a muxer. They take what the
// catalogue chose and turn it into elements, which is why adding AV1 or a new
// GPU compositor is a block in `codecs.toml` and not a change here.
// ---------------------------------------------------------------------------

/// The running configuration in the units the catalogue's `{unit, from}`
/// properties name.
fn encoder_vars(cfg: &Config) -> crate::catalogue::apply::Vars {
    let fps = cfg.canvas.fps.max(1) as i64;
    crate::catalogue::apply::Vars {
        video_bitrate_kbps: cfg.program.video_bitrate_kbps as i64,
        audio_bitrate_kbps: cfg.program.audio_bitrate_kbps as i64,
        keyframe_secs: cfg.program.keyframe_interval_secs as i64,
        keyframe_frames: fps * cfg.program.keyframe_interval_secs as i64,
        fps,
        cpu_count: crate::catalogue::apply::cpu_count(),
    }
}

/// The caps the compositor's output is pinned to.
///
/// With the software entry this is the canvas contract unchanged: I420 at
/// canvas size, colorimetry pinned, which is what every source is already
/// converted to. With a GPU entry the format is left to the backend and only
/// the geometry and the memory type are pinned, because a GL or CUDA
/// compositor picks its own internal format and forcing I420 on it would send
/// the frame back through system memory to satisfy a caps filter nobody
/// needed.
fn programme_caps(canvas: &CanvasCaps, gfx: &crate::catalogue::select::GraphicsChoice) -> gst::Caps {
    let Some(feature) = memory_feature(&gfx.memory) else {
        return canvas.video();
    };
    gst::Caps::builder("video/x-raw")
        .features([feature])
        .field("width", canvas.width)
        .field("height", canvas.height)
        .field("framerate", canvas.fps)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build()
}

fn memory_feature(memory: &str) -> Option<&'static str> {
    Some(match memory {
        "gl" => "memory:GLMemory",
        "cuda" => "memory:CUDAMemory",
        "va" => "memory:VAMemory",
        "d3d11" => "memory:D3D11Memory",
        "d3d12" => "memory:D3D12Memory",
        "vulkan" => "memory:VulkanImage",
        _ => return None,
    })
}

/// Everything between the raw programme tee and the encoder.
///
/// Software: one `videoconvert`, which is what this has always been. GPU: the
/// backend's own converter, and a download plus a `videoconvert` only when the
/// encoder cannot take frames in the compositor's memory. That last case is
/// the honest one on a Mac, where the compositor is GL and VideoToolbox wants
/// system memory; on an NVIDIA box with NVENC the frame never comes down.
fn encode_bridge(
    gfx: &crate::catalogue::select::GraphicsChoice,
    enc: &crate::catalogue::select::Chosen,
) -> Result<Vec<gst::Element>> {
    if !gfx.is_gpu() {
        return Ok(vec![make("videoconvert", "venc-conv")?]);
    }
    let mut out = Vec::new();
    if crate::probe::exists(&gfx.convert) {
        out.push(make(&gfx.convert, "venc-gconv")?);
    }
    if enc.memory != gfx.memory {
        out.extend(download_bridge(gfx, "venc")?);
    }
    Ok(out)
}

/// Bring frames back to system memory, for a branch that needs them there.
fn download_bridge(
    gfx: &crate::catalogue::select::GraphicsChoice,
    prefix: &str,
) -> Result<Vec<gst::Element>> {
    if !gfx.is_gpu() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    if let Some(d) = gfx.download.as_deref().filter(|d| crate::probe::exists(d)) {
        out.push(make(d, &format!("{prefix}-download"))?);
    }
    out.push(make("videoconvert", &format!("{prefix}-conv"))?);
    Ok(out)
}

/// Put a system memory picture into the compositor's memory, once.
fn upload_bridge(
    gfx: &crate::catalogue::select::GraphicsChoice,
    prefix: &str,
) -> Result<Vec<gst::Element>> {
    if !gfx.is_gpu() {
        return Ok(Vec::new());
    }
    match gfx.upload.as_deref().filter(|u| crate::probe::exists(u)) {
        Some(u) => Ok(vec![make(u, &format!("{prefix}-upload"))?]),
        None => Ok(Vec::new()),
    }
}

/// The parser the catalogue named for this codec, if it is installed.
///
/// `config-interval = -1` puts SPS and PPS in front of every keyframe, so a
/// player joining mid stream can decode without waiting for the next one. The
/// setter is defensive, so a parser for a codec that has no such thing simply
/// ignores it.
fn parser_for(name: Option<&str>, element_name: &str) -> Result<Vec<gst::Element>> {
    let Some(name) = name else { return Ok(Vec::new()) };
    if !crate::probe::exists(name) {
        warn!(parser = name, "the catalogue names a parser this machine does not have");
        return Ok(Vec::new());
    }
    let el = make(name, element_name)?;
    crate::probe::set_int(&el, "config-interval", -1);
    Ok(vec![el])
}

/// Link a tee to a chain of elements that were built as a list.
fn link_from(head: &gst::Element, chain: &[gst::Element]) -> Result<()> {
    let all: Vec<&gst::Element> = std::iter::once(head).chain(chain.iter()).collect();
    gst::Element::link_many(all)?;
    Ok(())
}

/// Compositor pads disagree about the width of `zorder` across backends, and
/// a mismatch is a panic rather than an error.
fn set_pad_u32(pad: &gst::Pad, prop: &str, v: u32) {
    if pad.find_property(prop).is_some() {
        pad.set_property(prop, v);
    }
}

impl Mixer {
    #[allow(clippy::type_complexity)]
    pub fn build(
        cfg: Config,
    ) -> Result<(Self, MixerHandle, mpsc::Receiver<Command>, mpsc::Receiver<BusEvent>)> {
        let canvas = CanvasCaps::new(&cfg.canvas);
        // What this machine will encode with, and what it will composite on,
        // comes from the catalogue rather than from names written here. That
        // is the whole point: AV1, or a GPU compositor, or a board nobody has
        // heard of yet, is an entry in codecs.toml and not a change to this
        // function.
        let sel = crate::catalogue::select(&cfg, None)?;
        crate::catalogue::log_selection(&sel);
        let vars = encoder_vars(&cfg);
        let backends = Backends::from_selection(&sel);
        backends.apply_decoder_ranks();

        let rt = tokio::runtime::Handle::try_current().context(
            "Mixer::build must be called from inside a Tokio runtime: the mixer \
             thread has none of its own and uses this handle to schedule retries",
        )?;

        let (tx, rx) = mpsc::channel(COMMAND_QUEUE);
        let (bus_tx, bus_rx) = mpsc::channel(BUS_QUEUE);
        let events = EventBus::new(256);
        let handle =
            MixerHandle { tx, events: events.clone(), coalesced: Arc::new(Coalesced::default()) };

        let program = gst::Pipeline::with_name("program");

        // --- video: mix, encode, fan out --------------------------------
        let gfx = &sel.graphics;
        let vmix = gstutil::make_live_aggregator(&gfx.compositor, "vmix")?;
        crate::probe::set_enum(&vmix, "background", "black");
        crate::probe::set_bool(&vmix, "ignore-inactive-pads", true);
        // Claim a fixed upstream latency up front.
        //
        // Attaching a branch to a running aggregator otherwise makes the
        // pipeline recalculate its latency, and everything downstream pauses
        // while it settles. Rolling an ad cost about a second of output that
        // way. Declaring the figure in advance means a later arrival changes
        // nothing.
        crate::probe::set_int(&vmix, "min-upstream-latency", MIN_UPSTREAM_LATENCY_NS);

        let vmix_caps = gstutil::capsfilter("vmix-caps", &programme_caps(&canvas, gfx))?;
        let vraw_tee = make("tee", "vraw-tee")?;
        vraw_tee.set_property("allow-not-linked", true);
        // Every programme frame passes this tee exactly once, before the
        // encoder and the multiview split apart, so it is where the frame
        // counter and the interval histogram go. See `observe::metrics`.
        crate::observe::attach_programme(&program, &vraw_tee);

        let venc = make(&sel.video_encode.element, "venc")?;
        crate::catalogue::apply::apply(&venc, &sel.video_encode.properties, &vars);
        crate::catalogue::apply::apply_keyframe(&venc, sel.video_encode.keyframe.as_ref(), &vars);
        let venc_tee = make("tee", "venc-tee")?;
        venc_tee.set_property("allow-not-linked", true);

        // queue, whatever it takes to get the canvas into the shape this
        // encoder wants, the encoder, its parser, the tee. With the software
        // graphics entry the bridge is one `videoconvert`, exactly as before;
        // with a GPU entry the frame has been on the GPU since the compositor
        // and comes down only if the encoder cannot take it there.
        let mut vchain: Vec<gst::Element> = vec![gstutil::queue_thread("venc-q")?];
        vchain.extend(encode_bridge(gfx, &sel.video_encode)?);
        vchain.push(venc.clone());
        vchain.extend(parser_for(sel.video_encode.parser.as_deref(), "vparse")?);
        vchain.push(venc_tee.clone());

        // Hold the video back by the audio encoder's uncompensated delay, so
        // what the viewer hears lines up with what they see. The figure is
        // `priming_delay_ms` on the audio entry. Applied at the encoder's own
        // pad: the raw programme tee for the multiview is not shifted, and the
        // muxer sees both streams already aligned.
        let av_offset_ns = match cfg.program.av_offset_ms {
            Some(ms) => ms * 1_000_000,
            None => (sel.audio_encode.priming_delay_ms.unwrap_or(0) * 1_000_000) as i64,
        };
        if let Some(sink) = venc.static_pad("sink") {
            sink.set_offset(av_offset_ns);
        }
        info!(
            audio_encoder = %sel.audio_encode.element,
            offset_ms = av_offset_ns / 1_000_000,
            "video held back to match the audio encoder's delay"
        );

        // Raw program video for the multiview's return cell. It leaves the
        // same tee, so on a GPU entry it is the one branch that comes back to
        // system memory.
        let pgm_video_proxy = make("proxysink", "pgm-v-proxy")?;
        let mut rchain: Vec<gst::Element> = vec![gstutil::queue_thread("pgm-v-q")?];
        rchain.extend(download_bridge(gfx, "pgm-v")?);
        rchain.push(make("videorate", "pgm-v-rate")?);
        rchain.push(make("videoscale", "pgm-v-scale")?);
        rchain.push(gstutil::capsfilter(
            "pgm-v-caps",
            &CanvasCaps::video_at(
                crate::input::THUMB_WIDTH,
                crate::input::THUMB_HEIGHT,
                gst::Fraction::new(cfg.multiview.fps.max(1), 1),
            ),
        )?);
        rchain.push(pgm_video_proxy.clone());

        // --- audio: mix, encode, fan out --------------------------------
        let amix = gstutil::make_live_aggregator("audiomixer", "amix")?;
        crate::probe::set_bool(&amix, "ignore-inactive-pads", true);
        crate::probe::set_int(&amix, "min-upstream-latency", MIN_UPSTREAM_LATENCY_NS);
        let amix_caps = gstutil::capsfilter("amix-caps", &canvas.audio())?;
        let araw_tee = make("tee", "araw-tee")?;
        araw_tee.set_property("allow-not-linked", true);

        let aenc = make(&sel.audio_encode.element, "aenc")?;
        crate::catalogue::apply::apply(&aenc, &sel.audio_encode.properties, &vars);
        let aenc_tee = make("tee", "aenc-tee")?;
        aenc_tee.set_property("allow-not-linked", true);

        // Defensive. The mixer is a live aggregator whose inputs are stamped
        // by another process against its own wall clock, so a contiguous
        // stream into the encoder is worth guaranteeing rather than assuming.
        // Added while chasing an on-air fault that turned out to live in the
        // RTMP server downstream, which re-serves AAC with its timeline
        // stepped backwards; this output, captured straight into ffmpeg,
        // measured clean without it. Kept because it costs nothing and closes
        // a gap that would otherwise be real the day an input drifts.
        let mut achain: Vec<gst::Element> = vec![
            gstutil::queue_thread("aenc-q")?,
            make("audioconvert", "aenc-conv")?,
            make("audiorate", "aenc-rate")?,
            aenc.clone(),
        ];
        achain.extend(parser_for(sel.audio_encode.parser.as_deref(), "aparse")?);
        achain.push(aenc_tee.clone());

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

        // On a GPU entry the slate is uploaded once here, like every other
        // source, and never comes back down.
        let mut slate_chain: Vec<gst::Element> = vec![slate.clone(), slate_caps.clone()];
        slate_chain.extend(upload_bridge(gfx, "slate")?);

        let fixed = [&vmix, &vmix_caps, &vraw_tee, &amix, &amix_caps, &level, &araw_tee, &silence, &silence_caps];
        let all: Vec<&gst::Element> = fixed
            .into_iter()
            .chain(vchain.iter())
            .chain(rchain.iter())
            .chain(achain.iter())
            .chain(slate_chain.iter())
            .collect();
        program.add_many(all).context("adding program elements")?;

        gst::Element::link_many([&vmix, &vmix_caps, &vraw_tee]).context("linking video mixer")?;
        link_from(&vraw_tee, &vchain).context("linking video encoder")?;
        link_from(&vraw_tee, &rchain).context("linking program return video")?;

        gst::Element::link_many([&amix, &amix_caps, &level, &araw_tee])
            .context("linking audio mixer")?;
        link_from(&araw_tee, &achain).context("linking audio encoder")?;

        gst::Element::link_many(slate_chain.iter().collect::<Vec<_>>())
            .context("linking slate")?;
        gst::Element::link(&silence, &silence_caps).context("linking silence")?;

        // Slate sits at the bottom of the z order, fully opaque, forever.
        let slate_pad = vmix.request_pad_simple("sink_%u").context("compositor refused slate pad")?;
        set_pad_u32(&slate_pad, "zorder", 0);
        slate_pad.set_property("alpha", 1.0f64);
        slate_chain
            .last()
            .and_then(|e| e.static_pad("src"))
            .context("the slate chain has no source pad")?
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
            programme_filters: Vec::new(),
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
            .push(gstutil::watch_bus(&self.program, gstutil::BusOwner::Programme, self.bus_tx.clone())?);
        self.program.set_state(gst::State::Playing).context("starting program pipeline")?;

        for src in self.cfg.sources.clone() {
            let id = src.id.clone();
            if let Err(e) = self.begin_add_source(src, None) {
                error!(source = %id, ?e, "failed to add source");
            }
        }

        // Filters on the programme itself. After the sources, because a filter
        // attached to one of them was put on at build time and this is only the
        // programme wide ones.
        for f in self.cfg.filters.clone() {
            if f.attach.source.is_some() || !f.attach.programme {
                continue;
            }
            if let Err(e) = self.add_programme_filter(&f) {
                error!(filter = %f.id, ?e, "failed to attach a programme filter");
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
        // A bare URI is still the ordinary way to write a source, but it is no
        // longer the only one: a source written as `type` plus `params` may
        // have no address at all (a capture card, a test pattern, an NDI name).
        // What it does need is a kind that can be resolved, and the resolver's
        // error names what this build has.
        anyhow::ensure!(
            !cfg.uri.trim().is_empty() || cfg.type_id.is_some(),
            "a source needs a uri, or a type saying what kind it is"
        );
        let provide = crate::plugin::source::resolve_config(cfg)?;
        cfg.validate_params()?;
        info!(
            source = %cfg.id,
            uri = %safe_uri_label(&cfg.display_uri()),
            kind = %provide.manifest.provide_id(),
            superimposed = overlay.is_some(),
            "adding source"
        );
        self.add_source_kind(cfg, true, overlay)?;
        self.persist_runtime();
        Ok(())
    }

    fn add_source_kind(
        &mut self,
        cfg: &SourceConfig,
        in_multiview: bool,
        overlay: Option<MediaReport>,
    ) -> Result<()> {
        if self.sources.iter().any(|s| s.input.id == cfg.id) {
            anyhow::bail!("source {} already exists", cfg.id);
        }
        let is_ad = cfg.id == AD_ID;
        // The thumbnail end is built only when there is a mosaic to put it in.
        // Nothing runs unless asked, and a source nobody is looking at should
        // not be scaling a picture for nobody. It can be attached later on a
        // running source without a rebuild; see `attach_thumb_end`.
        let wants_thumb = in_multiview && self.multiview.is_some();
        let input = InputPipeline::build_kind(
            cfg,
            &self.canvas,
            &self.backends,
            self.cfg.multiview.fps.max(1),
            self.origin,
            self.cfg.security.allow_exec_sources,
            &self.cfg.browser,
            overlay,
            wants_thumb,
        )?;

        // The operator's desk for this source, built once and handed over. See
        // `ProgrammeBranch`: the mixer no longer knows what is in it.
        let branch = ProgrammeBranch::build(
            &BranchCtx {
                program: &self.program,
                vmix: &self.vmix,
                amix: &self.amix,
                canvas: &self.canvas,
            },
            &cfg.id,
            &input.video_proxy,
            &input.audio_proxy,
            cfg.gain,
            cfg.muted,
        )?;

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
        // An `alpha` source composites its own layers on this pipeline's clock
        // and base time, so what it emits is already at the programme's running
        // time. Shifting that by the programme's age again put a source rebuilt
        // two minutes in two minutes into the future, where the compositor's
        // queue held its frames unconsumed and every later teardown waited on
        // the pad's stream lock for good.
        let aligner = if is_ad || input.composites_its_own_timeline() {
            None
        } else {
            Some(TimelineAligner::install(
                &self.program,
                &branch.vq,
                &branch.aq,
                &branch.vpad,
                &branch.apad,
                &cfg.id,
            )?)
        };

        // Filters this source was configured with on its input side, put on
        // before it starts, so the first frame out of it is already keyed. The
        // programme side ones need the branch registered first and go on below.
        let configured = self.configured_filters(&cfg.id);
        for f in configured.iter().filter(|f| f.attach.side == crate::config::FilterAttachSide::Input)
        {
            if let Err(e) = input.attach_filter(f, &self.canvas, false) {
                warn!(source = %cfg.id, filter = %f.id, ?e, "could not attach a configured filter");
            }
        }

        branch.sync_state();

        // An ad gets no mosaic tile. It would reshuffle the grid under the
        // operator mid-break, and the program return cell already shows it.
        if in_multiview {
            if let (Some(mv), Some(thumb)) = (&mut self.multiview, input.thumb_proxy()) {
                mv.add_tile(Some(cfg.id.clone()), &thumb).context("adding multiview tile")?;
            }
        }

        let watch = gstutil::watch_bus(
            &input.pipeline,
            gstutil::BusOwner::Source(cfg.id.clone()),
            self.bus_tx.clone(),
        )
        .context("watching input bus")?;
        self.adopt_clock(&input.pipeline);

        // Register the slot before starting, so that a failure to start can be
        // cleaned up by the ordinary removal path rather than leaking pads and
        // a live pipeline. A leaked failed ad blocked every later break and
        // kept posting its errors.
        self.sources.push(SourceSlot {
            input,
            branch,
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
        for f in configured
            .iter()
            .filter(|f| f.attach.side == crate::config::FilterAttachSide::Programme)
        {
            if let Err(e) = self.add_source_programme_filter(&cfg.id, f) {
                warn!(source = %cfg.id, filter = %f.id, ?e, "could not attach a configured filter");
            }
        }
        info!(source = %cfg.id, kind = %self.sources.last().map(|s| s.input.type_id()).unwrap_or_default(), "source added");
        self.broadcast_status();
        Ok(())
    }

    /// Put a filter where its `attach` says, while the programme runs.
    ///
    /// Four insertion points exist and this picks between them: a filter with a
    /// source goes on that source (its input side by default, its programme
    /// branch when `side = "programme"`), and one with `programme = true` goes
    /// on the programme before the tee every consumer reads, so the multiview
    /// and the encoder both see it.
    pub fn add_filter(&mut self, cfg: &crate::config::FilterConfig) -> Result<()> {
        anyhow::ensure!(!cfg.id.trim().is_empty(), "a filter needs an id");
        if let Some(source) = cfg.attach.source.clone() {
            let slot = self
                .sources
                .iter()
                .find(|s| s.input.id == source)
                .with_context(|| format!("no such source {source}"))?;
            match cfg.attach.side {
                crate::config::FilterAttachSide::Input => {
                    slot.input.attach_filter(cfg, &self.canvas, true)?;
                }
                crate::config::FilterAttachSide::Programme => {
                    self.add_source_programme_filter(&source, cfg)?;
                }
            }
            self.broadcast_status();
            return Ok(());
        }
        anyhow::ensure!(
            cfg.attach.programme,
            "a filter must say where it goes: attach = {{ source = \"cam1\" }} \
             or attach = {{ programme = true }}"
        );
        self.add_programme_filter(cfg)
    }

    /// The programme side point for one source: between its video queue and
    /// its compositor pad, and after the mute on the audio side so the meter
    /// stays honest. The thumbnail does not see it, which is the difference
    /// from the input side.
    fn add_source_programme_filter(
        &mut self,
        source: &SourceId,
        cfg: &crate::config::FilterConfig,
    ) -> Result<()> {
        anyhow::ensure!(
            !self.programme_filters.iter().any(|f| f.id() == cfg.id),
            "a filter called {} is already in place",
            cfg.id
        );
        let slot = self
            .sources
            .iter()
            .find(|s| &s.input.id == source)
            .with_context(|| format!("no such source {source}"))?;
        let filter = crate::plugin::filter::make(&cfg.type_id)?;
        let (upstream, pad) = if filter.stream() == crate::plugin::filter::Stream::Audio {
            (slot.branch.amute.clone(), slot.branch.apad.clone())
        } else {
            (slot.branch.vq.clone(), slot.branch.vpad.clone())
        };
        let mut params = cfg.params.clone();
        params
            .entry("id".to_string())
            .or_insert_with(|| toml::Value::String(format!("pgm-{source}-{}", cfg.id)));
        let placed = crate::plugin::filter::insert(
            crate::plugin::Insertion::before_pad(&self.program, &upstream, &pad),
            crate::plugin::FilterSpec {
                id: cfg.id.clone(),
                type_id: cfg.type_id.clone(),
                side: crate::plugin::FilterSide::SourceProgramme,
                params,
            },
            filter,
            &self.canvas,
            true,
        )?;
        self.programme_filters.push(placed);
        self.broadcast_status();
        Ok(())
    }

    /// The programme insertion point: between the compositor's capsfilter and
    /// the tee every consumer reads, so the encoder and the multiview both see
    /// the filtered picture.
    fn add_programme_filter(&mut self, cfg: &crate::config::FilterConfig) -> Result<()> {
        anyhow::ensure!(
            !self.programme_filters.iter().any(|f| f.id() == cfg.id),
            "the programme already has a filter called {}",
            cfg.id
        );
        let filter = crate::plugin::filter::make(&cfg.type_id)?;
        let (up, down) = if filter.stream() == crate::plugin::filter::Stream::Audio {
            ("pgm-level", "araw-tee")
        } else {
            ("vmix-caps", "vraw-tee")
        };
        let upstream = self
            .program
            .by_name(up)
            .with_context(|| format!("the programme has no element called {up}"))?;
        let downstream = self
            .program
            .by_name(down)
            .with_context(|| format!("the programme has no element called {down}"))?;
        let mut params = cfg.params.clone();
        params
            .entry("id".to_string())
            .or_insert_with(|| toml::Value::String(format!("pgm-{}", cfg.id)));
        let slot = crate::plugin::filter::insert(
            crate::plugin::Insertion::between(&self.program, &upstream, &downstream),
            crate::plugin::FilterSpec {
                id: cfg.id.clone(),
                type_id: cfg.type_id.clone(),
                side: crate::plugin::FilterSide::Programme,
                params,
            },
            filter,
            &self.canvas,
            true,
        )?;
        self.programme_filters.push(slot);
        self.broadcast_status();
        Ok(())
    }

    /// Change a filter wherever it is.
    fn set_filter(&mut self, id: &str, params: &crate::config::Params) -> FilterOutcome {
        let outcome = if let Some(slot) =
            self.programme_filters.iter_mut().find(|f| f.id() == id)
        {
            slot.configure(params)
        } else {
            let Some(source) =
                self.sources.iter().find(|s| s.input.filter_ids().iter().any(|f| f == id))
            else {
                return FilterOutcome::NoSuchFilter(format!(
                    "no filter called {id} on the programme or on any source"
                ));
            };
            source.input.configure_filter(id, params)
        };
        match outcome {
            Ok(crate::plugin::Configure::Applied) => FilterOutcome::Applied,
            Ok(crate::plugin::Configure::RestartRequired(why)) => {
                FilterOutcome::RestartRequired(why)
            }
            Err(e) => FilterOutcome::Failed(format!("{e:#}")),
        }
    }

    /// Take a filter out, wherever it is.
    fn remove_filter(&mut self, id: &str) -> Result<()> {
        if let Some(pos) = self.programme_filters.iter().position(|f| f.id() == id) {
            self.programme_filters.remove(pos).remove()?;
            self.broadcast_status();
            return Ok(());
        }
        let source = self
            .sources
            .iter()
            .find(|s| s.input.filter_ids().iter().any(|f| f == id))
            .with_context(|| {
                format!("no filter called {id} on the programme or on any source")
            })?;
        source.input.remove_filter(id)?;
        self.broadcast_status();
        Ok(())
    }

    fn filter_list(&self) -> Vec<FilterStatus> {
        let mut all: Vec<FilterStatus> = self
            .programme_filters
            .iter()
            .map(|f| FilterStatus {
                id: f.id().to_string(),
                type_id: f.spec.type_id.clone(),
                source: None,
                side: f.spec.side.as_str().to_string(),
            })
            .collect();
        for slot in &self.sources {
            for f in slot.input.filters() {
                all.push(FilterStatus {
                    id: f.0,
                    type_id: f.1,
                    source: Some(slot.input.id.clone()),
                    side: f.2,
                });
            }
        }
        all
    }

    /// Put a source pipeline on the programme's clock and base time.
    ///
    /// Separate pipelines otherwise each pick their own, so running times are
    /// not comparable across the proxy boundary. Live RTMP inputs get away with
    /// it because their timing comes from arrival, but a file's timestamps
    /// start at zero, and a compositor judging them against a programme that
    /// has been up for minutes sees them as ancient history.
    fn adopt_clock(&self, pipeline: &gst::Pipeline) {
        let Some(clock) = self.program.clock() else { return };
        pipeline.use_clock(Some(&clock));
        // start-time NONE stops the pipeline resetting base time when it
        // changes state, which would undo the line below.
        pipeline.set_start_time(gst::ClockTime::NONE);
        if let Some(base) = self.program.base_time() {
            pipeline.set_base_time(base);
        }
    }

    /// The configured filters that name one source.
    fn configured_filters(&self, id: &SourceId) -> Vec<crate::config::FilterConfig> {
        self.cfg
            .filters
            .iter()
            .filter(|f| f.attach.source.as_deref() == Some(id.as_str()))
            .cloned()
            .collect()
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
        slot.branch.vpad.set_property("zorder", 1u32);
        slot.branch.vpad.set_property("alpha", 1.0f64);
        slot.branch.apad.set_property("volume", 0.0f64);
        // A meter keeps the name of the source it was built for, and the
        // replacement builds one with the same name. Silenced here so that the
        // two cannot both be read as the new source's level.
        for el in &slot.branch.elements {
            if el.name() == slot.branch.meter.as_str() {
                crate::probe::set_bool(el, "post-messages", false);
            }
        }
        info!(source = %id, "source stopped, its last frame held on the programme");
        self.retired.push(RetiredBranch {
            id: id.clone(),
            vpad: slot.branch.vpad,
            apad: slot.branch.apad,
            branch: slot.branch.elements,
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
        let (vq_buffers, vq_time_ms) = level(Some(&slot.branch.vq));
        let (aq_buffers, aq_time_ms) = level(Some(&slot.branch.aq));
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
        for el in &slot.branch.elements {
            let _ = el.set_state(gst::State::Null);
            let _ = self.program.remove(el);
        }
        self.vmix.release_request_pad(&slot.branch.vpad);
        self.amix.release_request_pad(&slot.branch.apad);
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

            slot.branch.vpad.set_property("alpha", if on { 1.0f64 } else { 0.0f64 });
            slot.branch.vpad.set_property("zorder", if is_program { 2u32 } else { 1u32 });
            targets.push((slot.branch.apad.clone(), if on { 1.0f64 } else { 0.0f64 }));
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

        // An ad goes out at the level the file was made at. There is no
        // operator fader for it: it is not in the source list, so nothing draws
        // one, and a break that went out silent because the last source's fader
        // happened to be down would be worse than useless.
        let mut cfg = SourceConfig::bare(AD_ID, &crate::input::to_uri(&uri));
        cfg.name = Some("Ad break".into());
        // An ad is a file, always, whatever the address looks like: it ends
        // with EOS and that is how the break knows to return.
        cfg.type_id = Some(crate::plugin::kinds::file::MANIFEST.provide_id());
        cfg.stall_timeout_secs = f64::MAX; // An ad ends with EOS, never a stall.
        if let Err(e) = self.add_source_kind(&cfg, false, None) {
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
        slot.branch.vpad.set_offset(cue.nseconds() as i64);
        slot.branch.apad.set_offset(cue.nseconds() as i64);
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

    /// A name for one command, for a log line and for the alert after a
    /// panic. Cheap: no ids, no URIs, nothing that would put a stream key in a
    /// log.
    pub fn label(cmd: &Command) -> &'static str {
        match cmd {
            Command::Take { .. } => "program.take",
            Command::AdBreak { .. } => "adbreak.start",
            Command::EndAdBreak(_) => "adbreak.end",
            Command::AddSource(..) | Command::AddSourceProbed(..) => "source.add",
            Command::RemoveSource(..) => "source.remove",
            Command::ReconnectOutput(..) => "output.reconnect",
            Command::AddOutput(..) => "output.add",
            Command::RemoveOutput(..) => "output.remove",
            Command::RestartSource(_) => "source.restart",
            Command::SetAudio { .. } => "source.audio.set",
            Command::Seek { .. } => "source.seek",
            Command::AddFilter(..) => "filter.add",
            Command::SetFilter { .. } => "filter.set",
            Command::RemoveFilter(..) => "filter.remove",
            Command::ListFilters(_) => "filter.list",
            Command::Status(_) => "core.status",
            Command::Configs(_) => "core.configs",
            Command::Bus(_) => "bus message",
            Command::Tick => "tick",
            Command::PositionTick => "position tick",
            Command::Multiview(_) => "multiview demand",
            Command::Shutdown => "core.shutdown",
        }
    }

    /// Say on the event stream that a command failed in a way nothing planned
    /// for, so an operator watching the UI sees it rather than reading logs.
    pub fn alert(&self, severity: Severity, message: String) {
        let _ = self.events.send(Event::Alert { severity, message });
    }

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
                // Which way a source comes back is its own declaration, not a
                // flag on the core's struct. A kind without `restart-in-place`
                // is built again from nothing.
                if self.sources.iter().any(|s| s.input.id == id && !s.input.restarts_in_place()) {
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
            Command::AddFilter(cfg, ack) => {
                let r = self.add_filter(&cfg);
                reply(ack, &r);
                r?;
            }
            Command::SetFilter { id, params, reply } => {
                let _ = reply.send(self.set_filter(&id, &params));
            }
            Command::RemoveFilter(id, ack) => {
                let r = self.remove_filter(&id);
                reply(ack, &r);
                r?;
            }
            Command::ListFilters(reply) => {
                let _ = reply.send(self.filter_list());
            }
            Command::Status(reply) => {
                let _ = reply.send(self.status());
            }
            Command::Configs(reply) => {
                let _ = reply.send(self.runtime_configs());
            }
            Command::Bus(ev) => self.on_bus(ev),
            Command::Tick => {
                // Cleared before the work, so a tick that fires while this one
                // is running still queues the next.
                self.handle.coalesced.tick.store(false, Ordering::SeqCst);
                self.tick()
            }
            Command::PositionTick => {
                self.handle.coalesced.position.store(false, Ordering::SeqCst);
                self.position_tick()
            }
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

                // A source failing is contained in its own pipeline. Who posted
                // this is declared when the watcher is installed, not read back
                // out of a name.
                if let Some(id) = pipeline.source() {
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
                } else if let Some(slot) = self.sources.iter().find(|s| s.branch.owns_meter(&src)) {
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
                if pipeline.source() == Some(AD_ID) {
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
                if let Some(id) = pipeline.source() {
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
        // A source that does not declare `restart-in-place` is built again from
        // nothing (see `rebuild_source`), which for a page costs a browser
        // launch, a profile directory and about ten seconds of the operator's
        // attention. Doing that every twelve seconds for two hours is what
        // happened on 2026-09-11 and 2026-09-12, so it gets a policy of its
        // own. The decision comes from the capability the kind declared at
        // `initialize`, so a plugin that can come back in place says so and
        // gets the cheap path without the core knowing what it is.
        if !slot.input.restarts_in_place() {
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
                let mut status = SourceStatus {
                    id: s.input.id.clone(),
                    name: s.input.config.display_name().to_string(),
                    uri: safe_uri_label(&s.input.config.display_uri()),
                    state: s.input.observed_state(),
                    has_video: s.input.has_video(),
                    has_audio: s.input.has_audio(),
                    cell: cells.get(s.input.id.as_str()).copied(),
                    video_idle_ms: s.input.health.video_idle_ms(),
                    audio_idle_ms: s.input.health.audio_idle_ms(),
                    extra: Default::default(),
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
                };
                // What only this kind has. The keys are the same ones the JSON
                // always carried, written from the extras rather than from
                // fields on the universal shape.
                if s.input.superimposed() {
                    status.put_extra("superimposed", true);
                }
                // Only a source with separate sounds has levels, so this is
                // absent for everything else and the UI draws no faders for it.
                if let Some(levels) = s.input.levels() {
                    status.put_extra("audio", levels.report());
                }
                let filters = s.input.filter_ids();
                if !filters.is_empty() {
                    status.put_extra("filters", filters);
                }
                status
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
                    gstutil::BusOwner::Multiview,
                    self.bus_tx.clone(),
                )?);
                for slot in &self.sources {
                    if slot.input.id == AD_ID {
                        continue;
                    }
                    // A source pays for its thumbnail branch only while a
                    // mosaic exists to show it; the end is attached to the
                    // running source here and taken back at teardown.
                    let proxy = slot
                        .input
                        .attach_thumb_end(&self.canvas, shape.fps)
                        .context("attaching a thumbnail end for the mosaic")?;
                    mv.add_tile(Some(slot.input.id.clone()), &proxy)
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
                for slot in &self.sources {
                    slot.input.detach_thumb_end();
                }
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
    mut rx: mpsc::Receiver<Command>,
    handle: MixerHandle,
) -> std::thread::JoinHandle<()> {
    let ticker = handle.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK);
        loop {
            interval.tick().await;
            // Coalesced: a mixer held up by a slow state change comes back to
            // one tick, not to however many fired while it was busy.
            if let Err(e) = ticker.tick() {
                if e.downcast_ref::<Busy>().is_none() {
                    return;
                }
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
            if let Err(e) = positions.position_tick() {
                if e.downcast_ref::<Busy>().is_none() {
                    return;
                }
            }
        }
    });

    std::thread::Builder::new()
        .name("mixer".into())
        .spawn(move || {
            while let Some(cmd) = rx.blocking_recv() {
                let label = Mixer::label(&cmd);
                // A net under every command. The programme's encoder lives in
                // GStreamer's own threads and a panic here does not stop it,
                // but without this the mixer thread dies and every command
                // after it is refused for the life of the process: the show
                // stays on air and nothing can be changed. One command failing
                // must not cost the other twenty.
                //
                // `AssertUnwindSafe` is the honest spelling: the mixer is
                // `&mut` and a handler that panicked halfway may have left a
                // pad requested and unlinked. That is why the alert says the
                // state is uncertain rather than pretending nothing happened.
                let outcome =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| mixer.handle(cmd)));
                match outcome {
                    Ok(Ok(true)) => {}
                    Ok(Ok(false)) => break,
                    Ok(Err(e)) => warn!(?e, "command failed"),
                    // The panic message itself has already gone to the log
                    // through the default hook. The caller's reply channel was
                    // dropped in the unwind, so it is already getting an error.
                    Err(_) => {
                        error!(command = label, "the mixer panicked handling a command");
                        mixer.alert(
                            Severity::Error,
                            format!(
                                "the mixer failed while handling {label} and the command did \
                                 not complete. The programme is still on air. Check whatever \
                                 you just changed; restart the mixer when you can."
                            ),
                        );
                    }
                }
            }
            mixer.shutdown();
        })
        .expect("spawning mixer thread")
}

#[cfg(test)]
mod tests {
    use crate::plugin::branch::meter_name;
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
            codecs: Default::default(),
            media: Default::default(),
            security: Default::default(),
            browser: Default::default(),
            stall: Default::default(),
            sources: vec![],
            outputs: vec![],
            filters: vec![],
            plugins: Default::default(),
            tokens: vec![],
            extra: Default::default(),
        })
        .err()
        .expect("must refuse to build without a runtime");
        assert!(
            format!("{err:#}").contains("Tokio runtime"),
            "unhelpful error: {err:#}"
        );
    }

    fn programme_config(graphics: crate::config::Accel) -> crate::config::Config {
        let mut cfg = crate::config::Config {
            canvas: crate::config::Canvas {
                width: 320,
                height: 180,
                fps: 30,
                sample_rate: 48000,
                channels: 2,
            },
            program: Default::default(),
            multiview: Default::default(),
            snapshot: Default::default(),
            control: Default::default(),
            hardware: Default::default(),
            codecs: Default::default(),
            media: Default::default(),
            security: Default::default(),
            browser: Default::default(),
            stall: Default::default(),
            sources: vec![],
            outputs: vec![],
            filters: vec![],
            plugins: Default::default(),
            tokens: vec![],
            extra: Default::default(),
        };
        cfg.multiview.enabled = false;
        cfg.hardware.graphics = graphics;
        cfg
    }

    /// Build a programme, roll it, and count what reaches the encoder tee.
    /// Returns the number of encoded buffers seen and the first bus error.
    async fn roll_programme(cfg: crate::config::Config) -> Result<u64> {
        let (mix, _handle, _cmds, _bus) = Mixer::build(cfg)?;
        let seen = Arc::new(AtomicU64::new(0));
        let counter = seen.clone();
        let pad = mix.venc_tee.static_pad("sink").context("the encoder tee has no sink pad")?;
        pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            counter.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
        mix.program.set_state(gst::State::Playing).context("starting the programme")?;
        let bus = mix.program.bus().context("the programme has no bus")?;
        let mut failure = None;
        for _ in 0..100 {
            if seen.load(Ordering::Relaxed) > 0 {
                break;
            }
            if let Some(msg) = bus.timed_pop_filtered(
                gst::ClockTime::from_mseconds(50),
                &[gst::MessageType::Error],
            ) {
                if let gst::MessageView::Error(e) = msg.view() {
                    failure = Some(anyhow::anyhow!("{}", e.error()));
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let _ = mix.program.set_state(gst::State::Null);
        match failure {
            Some(e) => Err(e),
            None => Ok(seen.load(Ordering::Relaxed)),
        }
    }

    /// The software graphics entry is the default and is what a machine with
    /// no GPU, or with a GPU nobody has verified, gets. It must encode.
    #[tokio::test]
    async fn the_software_graphics_entry_runs_a_programme() {
        let _ = gst::init();
        let frames = roll_programme(programme_config(crate::config::Accel::Software))
            .await
            .expect("the software programme must run on every machine");
        assert!(frames > 0, "nothing reached the encoder");
    }

    /// And the GPU entry, pinned, builds the same programme with the frame
    /// uploaded once and composited on the GPU. Skipped where the elements are
    /// not installed, which is most CI runners.
    #[tokio::test]
    async fn the_gl_graphics_entry_runs_a_programme_when_it_is_pinned() {
        let _ = gst::init();
        if !crate::probe::exists("glvideomixer") || !crate::probe::exists("gldownload") {
            return;
        }
        let frames = roll_programme(programme_config(crate::config::Accel::Gl))
            .await
            .expect("the gl programme must run where the elements exist");
        assert!(frames > 0, "nothing reached the encoder through the GL compositor");
    }

    /// Pinning a graphics backend that is not here says so rather than
    /// quietly falling back, because an operator who pinned one wants to know.
    #[tokio::test]
    async fn pinning_a_graphics_backend_that_is_absent_fails_loudly() {
        let _ = gst::init();
        if crate::probe::exists("cudacompositor") {
            return;
        }
        let err = Mixer::build(programme_config(crate::config::Accel::Cuda))
            .err()
            .expect("cuda is not here and must be reported");
        let text = format!("{err:#}");
        assert!(text.contains("cuda"), "{text}");
        assert!(text.contains("software"), "the error must list what was available: {text}");
    }

    /// A panic inside one command must not end the mixer thread.
    ///
    /// `POST /api/sources {"uri":"test://bars"}` used to panic inside
    /// `set_property_from_str`, and with it went every command after it: the
    /// programme stayed on air with nothing able to change it. The pattern is
    /// checked now, but the net stays, because the next panic will be
    /// somewhere nobody predicted.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_panic_in_one_command_leaves_the_mixer_answering() {
        gst::init().expect("gstreamer");
        let (mut mix, handle, _cmds, _bus) =
            Mixer::build(programme_config(crate::config::Accel::Auto)).expect("mixer builds");
        let mut events = handle.subscribe();

        // The loop's guard, with a handler that panics standing in for one
        // that panicked by accident. The hook is quietened so the test output
        // is not a backtrace nobody is going to read.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Touching the mixer is the point: the unwind happens while it is
            // borrowed, which is what `AssertUnwindSafe` is asserting about.
            let _ = mix.status();
            panic!("a command went wrong");
        }));
        std::panic::set_hook(previous);
        assert!(outcome.is_err(), "the panic has to be caught, not propagated");

        // What the loop does next: say so, and carry on.
        mix.alert(Severity::Error, "the mixer failed while handling source.add".into());
        let envelope = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("an alert arrives")
            .expect("the bus is still open");
        match envelope.event {
            Event::Alert { severity, message } => {
                assert_eq!(severity, Severity::Error);
                assert!(message.contains("source.add"), "the alert names the command: {message}");
            }
            other => panic!("expected an alert, got {other:?}"),
        }

        // And the mixer still answers, which is the whole claim.
        assert!(mix.handle(Command::Tick).expect("a tick after the panic"));
        let status = mix.status();
        assert!(status.sources.is_empty(), "a status read still works");
    }

    /// Every command has a name, so the alert after a panic can say which one
    /// it was rather than "something".
    #[test]
    fn every_command_has_a_label() {
        let (tx, _rx) = oneshot::channel();
        for cmd in [
            Command::Tick,
            Command::PositionTick,
            Command::Shutdown,
            Command::Status(tx),
            Command::RemoveSource("cam1".into(), None),
            Command::RemoveFilter("key".into(), None),
        ] {
            let label = Mixer::label(&cmd);
            assert!(!label.is_empty() && !label.contains("cam1"), "{label}");
        }
    }

    /// A handle whose mixer thread is not running, so the queue fills and
    /// stays full. The receiver is kept alive: dropping it would make every
    /// send report a closed channel instead of a full one.
    fn parked_handle() -> (MixerHandle, mpsc::Receiver<Command>) {
        let (tx, rx) = mpsc::channel(COMMAND_QUEUE);
        let handle = MixerHandle {
            tx,
            events: EventBus::new(8),
            coalesced: Arc::new(Coalesced::default()),
        };
        (handle, rx)
    }

    /// The command queue was unbounded, so a client in a loop grew it without
    /// limit and the work still only happened at the rate one thread could do
    /// it. Full, the caller is told to come back and told when.
    #[test]
    fn a_full_command_queue_refuses_with_a_time_to_wait() {
        let (handle, _rx) = parked_handle();
        for i in 0..COMMAND_QUEUE {
            handle
                .send(Command::Take { source: None, at_running_time_ms: None, ack: None })
                .unwrap_or_else(|e| panic!("command {i} of the queue's own depth was refused: {e}"));
        }
        let err = handle
            .send(Command::Take { source: None, at_running_time_ms: None, ack: None })
            .expect_err("the queue is full and the next command must be refused");
        let busy = err.downcast_ref::<Busy>().expect("a full queue answers with Busy");
        assert_eq!(busy.queue, COMMAND_QUEUE);
        assert!(busy.retry_after_ms > 0, "a refusal has to say how long to wait");
        assert!(
            busy.to_string().contains("Nothing was changed"),
            "the message must say the mixer is unchanged: {busy}"
        );
    }

    /// Two timers firing while the mixer is busy must not both queue. A
    /// supervisor tick is worth doing once.
    #[test]
    fn repeated_ticks_coalesce_into_one() {
        let (handle, mut rx) = parked_handle();
        for _ in 0..20 {
            handle.tick().unwrap();
            handle.position_tick().unwrap();
        }
        let mut ticks = 0;
        let mut positions = 0;
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Command::Tick => ticks += 1,
                Command::PositionTick => positions += 1,
                other => panic!("unexpected {}", Mixer::label(&other)),
            }
        }
        assert_eq!((ticks, positions), (1, 1), "twenty fires queued more than one of each");
    }
}
