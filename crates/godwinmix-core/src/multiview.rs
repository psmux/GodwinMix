//! The operator's mosaic: every source plus a program return, composited into
//! one picture on the server and pushed to the UI as JPEG frames.
//!
//! Compositing server side is what keeps the control UI usable over a bad
//! remote link. Cost is one encode and one connection no matter how many
//! cameras are attached, instead of one of each per camera. The browser draws
//! a single image and overlays clickable regions on it.
//!
//! # Nothing runs unless asked
//!
//! The mosaic is the most expensive thing the core does for a client that may
//! not be there, so it is not built until somebody asks for it, and it is
//! taken down again when the last asker goes away.
//!
//! ```ignore
//! let sub = handle.subscribe(MultiviewRequest { fps: 8, width: 1280 });
//! while let Ok(frame) = sub.recv().await { /* one JPEG */ }
//! drop(sub); // the pipeline goes after the linger, if nobody else wants it
//! ```
//!
//! [`MultiviewHandle::subscribe`] takes a [`MultiviewRequest`] and hands back a
//! [`MultiviewSubscription`]. While at least one subscription is alive the
//! pipeline exists; the mosaic is built at the highest width and fps any
//! subscriber asked for. When the last subscription drops, a short linger
//! (`[multiview] linger_secs`, two seconds by default) runs before the
//! teardown, so a browser that reloads or a client that reconnects does not
//! pay for a rebuild. `subscribe` is the only way to get frames: the sender is
//! deliberately not public, so there is no path that reads the mosaic without
//! also keeping it alive.
//!
//! With `[multiview] enabled = false` the handle still exists and `subscribe`
//! still answers, but no pipeline is ever created, no thumbnail end is needed
//! on any source, and the subscription simply never yields a frame.
//!
//! # Why MJPEG and not WebRTC
//!
//! WebRTC would be smoother and could carry preview audio. It also needs SDP
//! negotiation, ICE, and a signalling path that survives whatever network the
//! operator is on. MJPEG over the WebSocket that the UI already holds open
//! needs none of that, works in every browser and in the Tauri webview on all
//! three desktop platforms, and keeps deployment to a single port.
//!
//! For picking which camera goes live, a 960x540 mosaic at 8 fps is enough.
//! Audio is covered separately by the level meters on the program bus. The
//! transport is deliberately behind a narrow interface so a WebRTC path can
//! replace it later without touching the mixer.
//!
//! This lives in its own pipeline so that a preview encoder falling over
//! cannot disturb the program path.

pub mod preview;

use crate::caps::{CanvasCaps, Grid};
use crate::config::MultiviewConfig;
use crate::gstutil::{self, make};
use crate::input::{THUMB_HEIGHT, THUMB_WIDTH};
use crate::state::{CellAssignment, MultiviewStatus, SourceId};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// Frames are dropped rather than queued when a viewer cannot keep up. A late
/// preview frame has no value, so the newest always wins.
const FRAME_CHANNEL_DEPTH: usize = 2;

/// Bounds on what a subscriber may ask for. A client asking for a 4K mosaic at
/// 60 fps would cost more than the programme it is previewing.
/// The name the preview's own cell on the mosaic carries, so a UI can label it
/// and a client asking for `/mjpeg/preview` is not told to add a source called
/// that. Reserved: a configured source may not use it.
pub const PREVIEW_CELL: &str = "__preview__";

const MIN_MOSAIC_WIDTH: i32 = 160;
const MAX_MOSAIC_WIDTH: i32 = 1920;
const MAX_MOSAIC_FPS: i32 = 30;

/// What one client wants out of the mosaic. The pipeline is built at the
/// highest of each field over every live subscription, so one agent asking for
/// 2 fps never degrades the operator's 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiviewRequest {
    pub fps: i32,
    /// Width of the whole mosaic, not of one cell.
    pub width: i32,
}

impl MultiviewRequest {
    /// What a client that just wants "whatever is configured" asks for. The
    /// web UI and the snapshot tracker both use this.
    pub fn configured() -> Self {
        Self { fps: 0, width: 0 }
    }
}

/// A request the mosaic is actually built at: clamped, even numbered, with the
/// height worked out from the configured aspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiviewShape {
    pub fps: i32,
    pub width: i32,
    pub height: i32,
}

/// What the handle asks the mixer to do. The mixer owns the pipeline because
/// it owns the source thumbnail ends the tiles are fed from; this module owns
/// the decision of whether there should be one at all.
#[derive(Debug, Clone, Copy)]
pub enum Demand {
    Build(MultiviewShape),
    Teardown,
    /// Reconcile the preview against what is subscribed: build it, rebuild it
    /// at another shape, or take it away. One verb rather than three, because
    /// the mixer thread is the only place where the count and the pipeline are
    /// both in hand and it can simply look.
    Preview,
}

/// A demand with the subscriber generation it was decided at.
///
/// Teardown used to be decided from a generation and an emptiness check taken
/// separately from the enqueueing, so a subscriber arriving between the two saw
/// a pipeline that was already condemned, enqueued nothing of its own, and then
/// lost the mosaic under it. The generation travels with the command now and
/// the mixer thread refuses one that is no longer current, reconciling the
/// count as it stands there instead of trusting the verb in the message.
#[derive(Debug, Clone, Copy)]
pub struct DemandAt {
    pub demand: Demand,
    pub generation: u64,
}

type DemandSink = Arc<dyn Fn(DemandAt) + Send + Sync>;

/// What the mosaic is doing, for `/metrics` and for anybody who wants to know
/// without holding a subscription. `gmx_multiview_subscribers` is
/// [`MultiviewStats::subscribers`] and `gmx_multiview_fps` is
/// [`MultiviewStats::fps`]; both are zero when no mosaic is running, which is
/// the honest answer rather than a stale last value.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MultiviewStats {
    /// Whether a client is allowed to ask at all, `[multiview] enabled`.
    pub enabled: bool,
    /// Whether a pipeline exists right now.
    pub built: bool,
    pub subscribers: u64,
    /// Frames per second over the life of the current pipeline.
    pub fps: f64,
    pub width: i32,
    pub height: i32,
    /// Frames published since the current pipeline was built.
    pub frames: u64,
}

struct Shared {
    cfg: MultiviewConfig,
    /// Stable across rebuilds, so a client holding a receiver keeps it when
    /// the mosaic is torn down and built again at another size.
    frames: broadcast::Sender<Arc<[u8]>>,
    subs: Mutex<Vec<(u64, MultiviewRequest)>>,
    next_id: AtomicU64,
    /// `gmx_multiview_subscribers`.
    subscribers: AtomicU64,
    /// Frames published since the pipeline was built, and when that was, which
    /// together give `gmx_multiview_fps`.
    frames_out: AtomicU64,
    since: Mutex<Option<Instant>>,
    built: AtomicBool,
    shape: Mutex<Option<MultiviewShape>>,
    /// Bumped by every subscribe and every drop, so a linger that was armed
    /// before a reconnection knows it has been overtaken.
    generation: AtomicU64,
    /// Mosaic pipelines alive for this mixer: one while there is a mosaic,
    /// zero otherwise. Counted on the `Multiview` object rather than taken
    /// from `built`, so a test can prove that the GStreamer elements really
    /// have gone and not merely that a flag was cleared.
    live: AtomicUsize,
    /// The mixer thread has no runtime of its own and a subscription may be
    /// dropped anywhere, so the linger is scheduled through a captured handle.
    rt: tokio::runtime::Handle,
    demand: Option<DemandSink>,
    /// Preview frames, on their own channel: a client watching the armed scene
    /// is not watching the mosaic and should not be sent both.
    preview_frames: broadcast::Sender<Arc<[u8]>>,
    /// Who wants a preview and at what shape, counted exactly as the mosaic's
    /// subscribers are. Empty means no preview compositor exists.
    preview_subs: Mutex<Vec<(u64, PreviewRequest)>>,
    preview_built: Mutex<Option<crate::multiview::preview::PreviewShape>>,
}

impl Shared {
    /// Say what is wanted, stamped with the generation it was decided at.
    fn ask(&self, demand: Demand) {
        let generation = self.generation.load(Ordering::SeqCst);
        self.ask_at(DemandAt { demand, generation });
    }

    fn ask_at(&self, d: DemandAt) {
        if let Some(sink) = &self.demand {
            sink(d);
        }
    }

    /// The shape the live subscriptions add up to, or `None` when there are
    /// none.
    fn wanted(&self) -> Option<MultiviewShape> {
        let subs = self.subs.lock();
        if subs.is_empty() {
            return None;
        }
        let fps = subs.iter().map(|(_, r)| r.fps).max().unwrap_or(0);
        let width = subs.iter().map(|(_, r)| r.width).max().unwrap_or(0);
        Some(self.shape_of(MultiviewRequest { fps, width }))
    }

    fn shape_of(&self, req: MultiviewRequest) -> MultiviewShape {
        let fps = if req.fps <= 0 { self.cfg.fps } else { req.fps };
        let width = if req.width <= 0 { self.cfg.width } else { req.width };
        let fps = fps.clamp(1, MAX_MOSAIC_FPS);
        let width = even(width.clamp(MIN_MOSAIC_WIDTH, MAX_MOSAIC_WIDTH));
        // Keep the configured aspect: the UI draws click regions over the
        // picture and a squashed one would put them in the wrong place.
        let height = even(
            ((width as i64 * self.cfg.height.max(1) as i64) / self.cfg.width.max(1) as i64) as i32,
        )
        .max(2);
        MultiviewShape { fps, width, height }
    }

    /// Ask the mixer for the shape the live subscriptions add up to, if that
    /// is not what is already running. Called after every change to the list
    /// that leaves at least one subscriber on it; an empty list is the
    /// business of the linger in `MultiviewSubscription::drop`, which needs an
    /// owned `Arc` that this cannot make from `&self`.
    fn settle(&self) {
        let Some(shape) = self.wanted() else { return };
        if *self.shape.lock() != Some(shape) || !self.built.load(Ordering::Acquire) {
            self.ask(Demand::Build(shape));
        }
    }

    /// Whether a command that has reached the mixer thread is still the truth.
    /// Anything that changed the subscriber list since it was decided has
    /// bumped the generation and put its own command on the queue behind this
    /// one, so the stale one is dropped and the fresh one decides.
    fn current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }

    /// The preview shape the live subscriptions add up to, or `None` when
    /// nobody is watching one.
    ///
    /// `full` wins over a size: one client asking for the canvas gets it, and
    /// the mosaic sized watchers see the same picture scaled, which is the
    /// same rule the mosaic's own width follows.
    fn preview_wanted(&self, canvas: &CanvasCaps) -> Option<preview::PreviewShape> {
        let subs = self.preview_subs.lock();
        if subs.is_empty() {
            return None;
        }
        let fps = subs.iter().map(|(_, r)| r.fps).max().unwrap_or(0);
        let fps = if fps <= 0 { self.cfg.fps } else { fps }.clamp(1, MAX_MOSAIC_FPS);
        if subs.iter().any(|(_, r)| r.full) {
            return Some(preview::PreviewShape::full(canvas, fps));
        }
        let width = subs.iter().map(|(_, r)| r.width).max().unwrap_or(0);
        let width = even(if width <= 0 { self.cfg.width / 2 } else { width }
            .clamp(MIN_MOSAIC_WIDTH, MAX_MOSAIC_WIDTH));
        let height = even(
            ((width as i64 * self.cfg.height.max(1) as i64) / self.cfg.width.max(1) as i64) as i32,
        )
        .max(2);
        Some(preview::PreviewShape::at(width, height, fps))
    }
}

/// What one client wants of the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PreviewRequest {
    pub fps: i32,
    pub width: i32,
    /// `ext.preview = "full"`: composited at the canvas's own size.
    pub full: bool,
}

fn even(v: i32) -> i32 {
    v - (v % 2)
}

/// The public face of the mosaic. Cloneable, cheap, and safe to hold whether
/// or not multiview is enabled.
#[derive(Clone)]
pub struct MultiviewHandle {
    shared: Arc<Shared>,
}

impl MultiviewHandle {
    /// Build a handle for a mixer. `demand` is how this module asks the mixer
    /// thread to create or destroy the pipeline; the mixer passes a closure
    /// that puts a `Command::Multiview` on its own queue, so pipeline work
    /// still happens on the one thread that owns GStreamer state changes.
    pub fn new(cfg: MultiviewConfig, rt: tokio::runtime::Handle, demand: DemandSink) -> Self {
        Self::with_sink(cfg, rt, Some(demand))
    }

    /// A handle attached to nothing: it counts subscribers and hands out
    /// receivers, but no pipeline is ever asked for. Used by `gmx bench` for
    /// the disabled rows and by tests.
    pub fn detached(cfg: MultiviewConfig, rt: tokio::runtime::Handle) -> Self {
        Self::with_sink(cfg, rt, None)
    }

    fn with_sink(
        cfg: MultiviewConfig,
        rt: tokio::runtime::Handle,
        demand: Option<DemandSink>,
    ) -> Self {
        let (frames, _) = broadcast::channel(FRAME_CHANNEL_DEPTH);
        let (preview_frames, _) = broadcast::channel(FRAME_CHANNEL_DEPTH);
        Self {
            shared: Arc::new(Shared {
                cfg,
                frames,
                preview_frames,
                preview_subs: Mutex::new(Vec::new()),
                preview_built: Mutex::new(None),
                subs: Mutex::new(Vec::new()),
                next_id: AtomicU64::new(1),
                subscribers: AtomicU64::new(0),
                frames_out: AtomicU64::new(0),
                since: Mutex::new(None),
                built: AtomicBool::new(false),
                shape: Mutex::new(None),
                generation: AtomicU64::new(0),
                live: AtomicUsize::new(0),
                rt,
                demand,
            }),
        }
    }

    pub fn enabled(&self) -> bool {
        self.shared.cfg.enabled
    }

    /// Whether any source needs a thumbnail end right now.
    ///
    /// The mosaic is the only consumer of a source's thumbnail branch, so a
    /// source built while this is false can leave that branch out entirely.
    /// See the note in `input.rs` about `Source::start(.., thumb)`.
    pub fn wants_thumbs(&self) -> bool {
        self.enabled() && !self.shared.subs.lock().is_empty()
    }

    /// Watch the armed scene.
    ///
    /// Asking for a preview is also asking for the mosaic, because the preview
    /// is composited from the mosaic's thumbnails: the subscription holds both
    /// up and gives both back. A client that wants only the preview does not
    /// have to know that, which is what `ext.preview` promises.
    pub fn subscribe_preview(&self, req: PreviewRequest) -> PreviewSubscription {
        let id = self.shared.next_id.fetch_add(1, Ordering::SeqCst);
        let frames = self.shared.preview_frames.subscribe();
        self.shared.preview_subs.lock().push((id, req));
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        let mosaic = self.subscribe(MultiviewRequest::configured());
        self.shared.ask(Demand::Preview);
        PreviewSubscription { id, shared: self.shared.clone(), frames, _mosaic: mosaic }
    }

    /// The preview shape the subscriptions add up to, for the mixer thread.
    pub fn preview_wanted(&self, canvas: &CanvasCaps) -> Option<preview::PreviewShape> {
        self.shared.preview_wanted(canvas)
    }

    /// What the preview is actually built at, or `None` when there is none.
    pub fn preview_built(&self) -> Option<preview::PreviewShape> {
        *self.shared.preview_built.lock()
    }

    pub fn mark_preview_built(&self, shape: Option<preview::PreviewShape>) {
        *self.shared.preview_built.lock() = shape;
    }

    /// How many clients are watching the armed scene.
    pub fn preview_subscribers(&self) -> u64 {
        self.shared.preview_subs.lock().len() as u64
    }

    /// Where a built preview publishes its frames.
    pub fn preview_publisher(&self) -> impl Fn(Arc<[u8]>) + Send + Sync + 'static {
        let tx = self.shared.preview_frames.clone();
        move |frame| {
            let _ = tx.send(frame);
        }
    }

    /// How many clients are holding the mosaic up. `gmx_multiview_subscribers`.
    pub fn subscribers(&self) -> u64 {
        self.shared.subscribers.load(Ordering::Relaxed)
    }

    /// Frames per second measured over the life of the current pipeline, zero
    /// when there is none. `gmx_multiview_fps`.
    pub fn fps(&self) -> f64 {
        let Some(since) = *self.shared.since.lock() else { return 0.0 };
        let secs = since.elapsed().as_secs_f64();
        if secs <= 0.0 {
            return 0.0;
        }
        self.shared.frames_out.load(Ordering::Relaxed) as f64 / secs
    }

    /// Whether a mosaic pipeline exists at this moment.
    pub fn is_built(&self) -> bool {
        self.shared.built.load(Ordering::Acquire)
    }

    /// Mosaic pipelines alive for this mixer right now: one or zero. What a
    /// test asks to prove that a disabled or unwanted mosaic is not merely
    /// flagged off but absent.
    pub fn live_pipelines(&self) -> usize {
        self.shared.live.load(Ordering::Relaxed)
    }

    pub fn shape(&self) -> Option<MultiviewShape> {
        *self.shared.shape.lock()
    }

    /// One read for everything a metrics endpoint wants, so `/metrics` takes a
    /// snapshot rather than four separate atomics that could disagree with
    /// each other between lines.
    pub fn stats(&self) -> MultiviewStats {
        let shape = self.shape();
        MultiviewStats {
            enabled: self.enabled(),
            built: self.is_built(),
            subscribers: self.subscribers(),
            fps: self.fps(),
            width: shape.map(|s| s.width).unwrap_or(0),
            height: shape.map(|s| s.height).unwrap_or(0),
            frames: self.shared.frames_out.load(Ordering::Relaxed),
        }
    }

    /// Ask for the mosaic and hold it up for as long as the returned guard
    /// lives. The first subscriber builds the pipeline; the last one to drop
    /// takes it down again after the configured linger.
    pub fn subscribe(&self, req: MultiviewRequest) -> MultiviewSubscription {
        // The receiver is taken before the pipeline is asked for, so no frame
        // can be published between the two.
        let frames = self.shared.frames.subscribe();
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        if self.enabled() {
            self.shared.subs.lock().push((id, req));
            self.shared.subscribers.fetch_add(1, Ordering::Relaxed);
            self.shared.generation.fetch_add(1, Ordering::SeqCst);
            self.shared.settle();
        }
        MultiviewSubscription { shared: self.shared.clone(), id, frames }
    }

    /// Called by the mixer when the pipeline has been created or destroyed.
    pub fn mark_built(&self, shape: Option<MultiviewShape>) {
        *self.shared.shape.lock() = shape;
        self.shared.built.store(shape.is_some(), Ordering::Release);
        self.shared.frames_out.store(0, Ordering::Relaxed);
        *self.shared.since.lock() = shape.map(|_| Instant::now());
    }

    /// The shape the current demand implies, for the mixer to build at.
    pub fn wanted(&self) -> Option<MultiviewShape> {
        self.shared.wanted()
    }

    /// Whether a demand that has reached the mixer thread is still current.
    ///
    /// A subscriber that arrived after the demand was decided has bumped the
    /// generation and asked for what it wants, so the stale command is dropped
    /// rather than applied over the top of the fresh one.
    pub fn accepts(&self, d: DemandAt) -> bool {
        self.shared.current(d.generation)
    }

    /// The current subscriber generation, for a command the mixer thread
    /// raises itself.
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::SeqCst)
    }

    fn publisher(&self) -> Publisher {
        Publisher { shared: self.shared.clone() }
    }
}

/// What the appsink callback holds: the frame sender and the counters, without
/// the rest of the handle.
#[derive(Clone)]
struct Publisher {
    shared: Arc<Shared>,
}

impl Publisher {
    fn publish(&self, frame: Arc<[u8]>) {
        self.shared.frames_out.fetch_add(1, Ordering::Relaxed);
        // A send failure just means nobody is watching.
        let _ = self.shared.frames.send(frame);
    }
}

/// Proof that somebody wants the mosaic. Dropping it gives the pipeline up.
/// One client's hold on the preview. Dropping it is what eventually takes the
/// preview compositor away, and the mosaic with it if nothing else wants one.
pub struct PreviewSubscription {
    id: u64,
    shared: Arc<Shared>,
    frames: broadcast::Receiver<Arc<[u8]>>,
    /// The mosaic subscription the preview implies, held for the same life.
    _mosaic: MultiviewSubscription,
}

impl PreviewSubscription {
    pub async fn recv(&mut self) -> Result<Arc<[u8]>, broadcast::error::RecvError> {
        self.frames.recv().await
    }
}

impl Drop for PreviewSubscription {
    fn drop(&mut self) {
        self.shared.preview_subs.lock().retain(|(id, _)| *id != self.id);
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        self.shared.ask(Demand::Preview);
    }
}

pub struct MultiviewSubscription {
    shared: Arc<Shared>,
    id: u64,
    frames: broadcast::Receiver<Arc<[u8]>>,
}

impl MultiviewSubscription {
    /// The next mosaic frame. Pends forever when multiview is disabled, which
    /// is what a `select!` arm wants: no frames, no special case.
    pub async fn recv(&mut self) -> Result<Arc<[u8]>, broadcast::error::RecvError> {
        self.frames.recv().await
    }

    /// Whether this subscription can ever produce a frame.
    pub fn active(&self) -> bool {
        self.shared.cfg.enabled
    }
}

impl Drop for MultiviewSubscription {
    fn drop(&mut self) {
        if !self.shared.cfg.enabled {
            return;
        }
        let empty = {
            let mut subs = self.shared.subs.lock();
            subs.retain(|(id, _)| *id != self.id);
            subs.is_empty()
        };
        self.shared.subscribers.fetch_sub(1, Ordering::Relaxed);
        let gen = self.shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if !empty {
            // Somebody else is still watching, possibly at a smaller size.
            self.shared.settle();
            return;
        }
        let shared = self.shared.clone();
        let linger = Duration::from_secs(shared.cfg.linger_secs);
        shared.rt.clone().spawn(async move {
            tokio::time::sleep(linger).await;
            // A client that came back during the linger bumped the generation,
            // and its own settle has already asked for what it needs. This is
            // the cheap check; the one that matters is on the mixer thread,
            // because a client can arrive between here and there.
            if !shared.current(gen) {
                return;
            }
            if !shared.subs.lock().is_empty() {
                return;
            }
            info!("last multiview subscriber left, taking the mosaic down");
            // Stamped with the generation this decision was made at, not with
            // whatever it is by the time the mixer thread reads it.
            shared.ask_at(DemandAt { demand: Demand::Teardown, generation: gen });
        });
    }
}

struct Tile {
    /// None for the program return cell.
    source: Option<SourceId>,
    pad: gst::Pad,
    branch: Vec<gst::Element>,
    /// The end of the tile branch, carrying `allow-not-linked`, so the preview
    /// compositor can take the same picture without a second decode, a second
    /// scale or a second proxy. A tee with one branch has no thread and costs
    /// nothing, which is why it is always there rather than spliced in when a
    /// preview arrives.
    tee: gst::Element,
}

pub struct Multiview {
    cfg: MultiviewConfig,
    /// The armed scene, composited from the same thumbnails. Built when a
    /// client subscribes with `ext.preview` and taken down after.
    preview: Option<preview::ScenePreview>,
    /// Held so the live count falls when this is dropped.
    shared: Arc<Shared>,
    pipeline: gst::Pipeline,
    compositor: gst::Element,
    tiles: Vec<Tile>,
    grid: Grid,
    /// Held so the bus watch for this pipeline dies with it.
    watch: Option<gstutil::BusWatch>,
}

impl Multiview {
    /// Build the mosaic at `shape`, publishing frames through `handle`.
    pub fn build(
        handle: &MultiviewHandle,
        shape: MultiviewShape,
        program_video: &gst::Element,
    ) -> Result<Self> {
        let mut cfg = handle.shared.cfg.clone();
        cfg.width = shape.width;
        cfg.height = shape.height;
        cfg.fps = shape.fps;

        let pipeline = gst::Pipeline::with_name("multiview");
        crate::observe::register_pipeline("multiview", &pipeline);
        let fps = gst::Fraction::new(cfg.fps.max(1), 1);

        // force-live and ignore-inactive-pads together make the mosaic tick
        // along on its own schedule even when every camera is dead. Without
        // them a stalled input would freeze the operator's whole view.
        let compositor = gstutil::make_live_aggregator("compositor", "mv-comp")?;
        compositor.set_property_from_str("background", "black");
        crate::probe::set_bool(&compositor, "ignore-inactive-pads", true);
        // Start the output where the first tile is, not at zero.
        //
        // The mosaic is built when a client asks for it, which may be an hour
        // into the broadcast, and the tiles arrive over a proxy carrying the
        // programme's running time. An aggregator whose output starts at zero
        // then has an hour of mosaic to cover before it reaches the present,
        // and it covers it as fast as the machine allows: an 8 fps mosaic on a
        // mixer that had been up three seconds opened with two dozen identical
        // frames, all encoded, all pushed at the websocket. Ten minutes in it
        // would have been five thousand.
        compositor.set_property_from_str("start-time-selection", "first");

        let vcaps = gstutil::capsfilter(
            "mv-caps",
            &CanvasCaps::video_at(cfg.width, cfg.height, fps),
        )?;
        let vqueue = gstutil::queue_preview("mv-q")?;
        let vconv = make("videoconvert", "mv-conv")?;
        let enc = make("jpegenc", "mv-jpeg")?;
        crate::probe::set_int(&enc, "quality", cfg.jpeg_quality as i64);

        let sink = gst_app::AppSink::builder()
            .name("mv-sink")
            // Never build a backlog. The freshest frame is the only useful one.
            .max_buffers(1)
            .drop(true)
            .sync(false)
            .build();

        {
            let publisher = handle.publisher();
            sink.set_callbacks(
                gst_app::AppSinkCallbacks::builder()
                    .new_sample(move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                        let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                        publisher.publish(Arc::from(map.as_slice()));
                        Ok(gst::FlowSuccess::Ok)
                    })
                    .build(),
            );
        }
        let sink_el: gst::Element = sink.upcast();

        pipeline
            .add_many([&compositor, &vcaps, &vqueue, &vconv, &enc, &sink_el])
            .context("adding multiview elements")?;
        gst::Element::link_many([&compositor, &vcaps, &vqueue, &vconv, &enc, &sink_el])
            .context("linking mosaic")?;

        let mut mv = Self {
            cfg,
            preview: None,
            shared: handle.shared.clone(),
            pipeline,
            compositor,
            tiles: Vec::new(),
            grid: Grid::for_tiles(1, shape.width, shape.height),
            watch: None,
        };
        mv.shared.live.fetch_add(1, Ordering::Relaxed);

        if mv.cfg.include_program {
            mv.add_tile(None, program_video)?;
        }
        Ok(mv)
    }

    /// The bus watch for this pipeline, held so that it dies with it.
    pub fn attach_watch(&mut self, watch: gstutil::BusWatch) {
        self.watch = Some(watch);
    }

    /// Share the programme's clock and base time.
    ///
    /// The mosaic is now built long after the programme started, and its tiles
    /// arrive over a proxy carrying the programme's running times. A pipeline
    /// that picked its own base time would judge those buffers against a
    /// timeline that began a moment ago: the compositor saw them as ancient,
    /// ran fast to catch up, and the mosaic came out at twice the framerate it
    /// was asked for. Same clock, same base time, same running times, and the
    /// measured fps matches the configured one.
    pub fn follow_clock_of(&self, programme: &gst::Pipeline) {
        let Some(clock) = programme.clock() else { return };
        self.pipeline.use_clock(Some(&clock));
        // start-time NONE stops the pipeline resetting base time when it
        // changes state, which would undo the line below.
        self.pipeline.set_start_time(gst::ClockTime::NONE);
        if let Some(base) = programme.base_time() {
            self.pipeline.set_base_time(base);
        }
    }

    /// Attach one more tile, fed from a `proxysink` in another pipeline.
    pub fn add_tile(&mut self, source: Option<SourceId>, proxy: &gst::Element) -> Result<()> {
        let tag = source.clone().unwrap_or_else(|| "program".into());

        let src = make("proxysrc", &format!("mv-src-{tag}"))?;
        src.set_property("proxysink", proxy);
        let queue = gstutil::queue_preview(&format!("mv-q-{tag}"))?;
        let rate = make("videorate", &format!("mv-rate-{tag}"))?;
        // Start at the first buffer that arrives, not at the start of the
        // segment. The mosaic is built when a client asks for it, which may be
        // an hour into the broadcast, and the tiles then arrive carrying the
        // programme's running time. Without this, videorate fills the gap
        // between the segment start and that first buffer with duplicates: an
        // 8 fps mosaic on a mixer that had been up ten minutes opened with
        // nearly five thousand identical frames, encoded and pushed at the
        // speed of the machine, before it settled to the rate it was asked
        // for.
        crate::probe::set_bool(&rate, "skip-to-first", true);
        let scale = make("videoscale", &format!("mv-scale-{tag}"))?;
        let caps = gstutil::capsfilter(
            &format!("mv-caps-{tag}"),
            &CanvasCaps::video_at(
                THUMB_WIDTH,
                THUMB_HEIGHT,
                gst::Fraction::new(self.cfg.fps.max(1), 1),
            ),
        )?;

        // `allow-not-linked` so the preview taking a branch, or giving one
        // back, is nothing to the tile: the mosaic keeps drawing whatever
        // happens on the other side.
        let tee = make("tee", &format!("mv-tee-{tag}"))?;
        tee.set_property("allow-not-linked", true);
        let branch = vec![src, queue, rate, scale, caps, tee.clone()];
        self.pipeline.add_many(&branch).context("adding tile branch")?;
        gst::Element::link_many(&branch).context("linking tile branch")?;

        let pad = self
            .compositor
            .request_pad_simple("sink_%u")
            .context("compositor refused a tile pad")?;
        // Letterbox rather than stretch, so a 4:3 camera beside a 16:9 one
        // still looks like itself.
        pad.set_property_from_str("sizing-policy", "keep-aspect-ratio");
        let tee_pad = tee.request_pad_simple("src_%u").context("a tile tee refused a pad")?;
        tee_pad.link(&pad).context("linking tile into the mosaic")?;

        for el in &branch {
            el.sync_state_with_parent().ok();
        }

        self.tiles.push(Tile { source, pad, branch, tee });
        self.relayout();
        debug!(%tag, "added multiview tile");
        Ok(())
    }

    pub fn remove_tile(&mut self, source: &SourceId) -> Result<()> {
        let Some(pos) = self
            .tiles
            .iter()
            .position(|t| t.source.as_deref() == Some(source.as_str()))
        else {
            return Ok(());
        };
        let tile = self.tiles.remove(pos);
        // The preview draws off this tile's tee, so its slot goes first or the
        // branch would be taken down under a linked pad.
        if let Some(preview) = self.preview.as_mut() {
            preview.drop_source(source);
        }
        // The mosaic's pad goes back before the branch feeding it is taken to
        // NULL, for the reason written out over `ScenePreview::unbind`: the
        // release flushes the pad, and without that flush an element on its
        // way to NULL waits for a streaming thread that is parked in the
        // compositor's chain function.
        self.compositor.release_request_pad(&tile.pad);
        for el in &tile.branch {
            // Locked first, so the bin's own state walk cannot put it back
            // to PLAYING before the remove. See `Encoder::detach`.
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
            let _ = self.pipeline.remove(el);
        }
        self.relayout();
        Ok(())
    }

    /// Recompute the grid and move every tile into its cell.
    ///
    /// Only pad properties change, so adding a camera mid-broadcast reshuffles
    /// the mosaic without rebuilding anything.
    fn relayout(&mut self) {
        let grid = Grid::for_tiles(self.tiles.len() as u32, self.cfg.width, self.cfg.height);
        self.grid = grid;
        for (i, tile) in self.tiles.iter().enumerate() {
            let (x, y) = grid.cell_origin(i as u32);
            tile.pad.set_property("xpos", x);
            tile.pad.set_property("ypos", y);
            tile.pad.set_property("width", grid.cell_w);
            tile.pad.set_property("height", grid.cell_h);
            tile.pad.set_property("zorder", if tile.source.is_none() { 1u32 } else { 0u32 });
        }
        info!(tiles = self.tiles.len(), cols = grid.cols, rows = grid.rows, "multiview relaid out");
    }

    // -- the armed scene ------------------------------------------------

    /// Build the preview compositor, or rebuild it at another shape.
    ///
    /// Idempotent: asked for a shape it already has, it does nothing. A
    /// different shape is a rebuild, because a compositor's output caps are
    /// fixed and there is no client watching a preview who would rather see it
    /// at the wrong size than wait a frame.
    pub fn preview_on(
        &mut self,
        shape: preview::PreviewShape,
        publish: impl Fn(Arc<[u8]>) + Send + Sync + 'static,
    ) -> Result<()> {
        if self.preview.as_ref().is_some_and(|p| p.shape() == shape) {
            return Ok(());
        }
        self.preview_off();
        let mut built =
            preview::ScenePreview::build(&self.pipeline, shape, publish, self.cfg.jpeg_quality as i32)?;
        // A tile of its own on the mosaic, so an operator watching the sheet
        // sees what is armed beside what is live without a second stream.
        match self.attach_preview_tile(&built) {
            Ok(pad) => built.set_tile_pad(Some(pad)),
            Err(e) => warn!(?e, "the preview has no tile on the mosaic; the stream still works"),
        }
        self.preview = Some(built);
        self.relayout();
        Ok(())
    }

    /// Take the preview away. Nothing is left: no compositor, no pads, no
    /// queues, and no tile on the mosaic.
    pub fn preview_off(&mut self) {
        let Some(mut preview) = self.preview.take() else { return };
        if let Some(pad) = preview.tile_pad().cloned() {
            if let Some(pos) = self.tiles.iter().position(|t| t.pad == pad) {
                let tile = self.tiles.remove(pos);
                // Pad back first, branch down second. Same order and same
                // reason as `remove_tile` above.
                self.compositor.release_request_pad(&tile.pad);
                for el in &tile.branch {
                    el.set_locked_state(true);
                    let _ = el.set_state(gst::State::Null);
                    let _ = self.pipeline.remove(el);
                }
            }
        }
        preview.teardown();
        self.relayout();
    }

    /// The preview's own cell on the mosaic.
    fn attach_preview_tile(&mut self, built: &preview::ScenePreview) -> Result<gst::Pad> {
        let queue = gstutil::queue_preview("mv-q-preview")?;
        let tee = make("tee", "mv-tee-preview")?;
        tee.set_property("allow-not-linked", true);
        let branch = vec![queue.clone(), tee.clone()];
        self.pipeline.add_many(&branch).context("adding the preview tile")?;
        let out = built
            .output()
            .request_pad_simple("src_%u")
            .context("the preview tee refused a pad for the mosaic")?;
        out.link(&queue.static_pad("sink").context("the preview tile queue has no sink pad")?)
            .context("linking the preview into its mosaic tile")?;
        queue.link(&tee).context("linking the preview tile")?;
        let pad = self
            .compositor
            .request_pad_simple("sink_%u")
            .context("the mosaic refused a pad for the preview")?;
        pad.set_property_from_str("sizing-policy", "keep-aspect-ratio");
        let tee_pad = tee.request_pad_simple("src_%u").context("the preview tee refused a pad")?;
        tee_pad.link(&pad).context("linking the preview into the mosaic")?;
        for el in &branch {
            el.sync_state_with_parent().ok();
        }
        self.tiles.push(Tile { source: Some(PREVIEW_CELL.into()), pad: pad.clone(), branch, tee });
        Ok(pad)
    }

    /// Draw the armed scene. Nothing to draw is a black preview, not an error:
    /// disarming a scene is a thing an operator does.
    pub fn apply_preview(&mut self, canvas: &CanvasCaps, cells: &[preview::Cell]) -> Result<()> {
        let tiles: Vec<(SourceId, gst::Element)> = self
            .tiles
            .iter()
            .filter_map(|t| t.source.clone().map(|id| (id, t.tee.clone())))
            .collect();
        let Some(preview) = self.preview.as_mut() else { return Ok(()) };
        preview.apply(canvas, cells, |source| {
            tiles.iter().find(|(id, _)| id == source).map(|(_, tee)| tee.clone())
        })
    }

    /// What the preview is composited at, or `None` when there is none.
    pub fn preview_shape(&self) -> Option<preview::PreviewShape> {
        self.preview.as_ref().map(|p| p.shape())
    }

    /// How many sources the preview is drawing, for a test and for `/metrics`.
    pub fn preview_drawn(&self) -> usize {
        self.preview.as_ref().map(|p| p.drawn()).unwrap_or(0)
    }

    /// Break the preview on purpose, for the test that proves it cannot reach
    /// air.
    ///
    /// Taking the preview's own compositor to NULL while its inputs are still
    /// pushing is about the worst thing that can happen to it: the branch
    /// errors, its queues fill and its pads refuse. The programme is two proxy
    /// boundaries away and the tile tees carry `allow-not-linked`, so the
    /// measurement is that nothing downstream of them moves at all.
    pub fn break_preview_for_a_test(&self) -> bool {
        let Some(preview) = self.preview.as_ref() else { return false };
        preview.break_for_a_test();
        true
    }

    pub fn start(&self) -> Result<()> {
        self.pipeline.set_state(gst::State::Playing).context("starting multiview")?;
        Ok(())
    }

    pub fn stop(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }

    pub fn pipeline(&self) -> &gst::Pipeline {
        &self.pipeline
    }

    pub fn shape(&self) -> MultiviewShape {
        MultiviewShape { fps: self.cfg.fps, width: self.cfg.width, height: self.cfg.height }
    }

    /// Everything the UI needs to draw clickable regions over the video.
    pub fn status(&self) -> MultiviewStatus {
        let cells = self
            .tiles
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let (x, y) = self.grid.cell_origin(i as u32);
                CellAssignment {
                    index: i as u32,
                    source: t.source.clone(),
                    x,
                    y,
                    w: self.grid.cell_w,
                    h: self.grid.cell_h,
                }
            })
            .collect();
        MultiviewStatus {
            enabled: self.cfg.enabled,
            width: self.cfg.width,
            height: self.cfg.height,
            cols: self.grid.cols,
            rows: self.grid.rows,
            cells,
            fps: self.cfg.fps,
        }
    }
}

impl Drop for Multiview {
    fn drop(&mut self) {
        self.stop();
        self.shared.live.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    /// The next frame a subscription delivers, inside `within`.
    ///
    /// The channel keeps the newest two frames and tells a reader that fell
    /// behind so, by design; a test that was not scheduled for a quarter of a
    /// second is such a reader and reads again rather than calling the
    /// channel closed.
    async fn next_frame(sub: &mut MultiviewSubscription, within: Duration) -> Arc<[u8]> {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(left, sub.recv()).await {
                Ok(Ok(frame)) => return frame,
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(broadcast::error::RecvError::Closed)) => panic!("the frame channel closed"),
                Err(_) => panic!("no mosaic frame within {within:?} of subscribing"),
            }
        }
    }
    use super::*;

    fn init() {
        let _ = gst::init();
    }

    fn handle() -> MultiviewHandle {
        MultiviewHandle::detached(
            MultiviewConfig::default(),
            tokio::runtime::Handle::current(),
        )
    }

    fn default_shape(h: &MultiviewHandle) -> MultiviewShape {
        h.shared.shape_of(MultiviewRequest::configured())
    }

    /// Stand-in for a proxysink living in another pipeline.
    fn fake_proxy(name: &str) -> gst::Element {
        let p = gst::Pipeline::with_name(&format!("fake-{name}"));
        let sink = make("proxysink", name).unwrap();
        p.add(&sink).unwrap();
        std::mem::forget(p);
        sink
    }

    #[tokio::test]
    async fn mosaic_starts_with_only_the_program_return() {
        init();
        let h = handle();
        let mv = Multiview::build(&h, default_shape(&h), &fake_proxy("pv")).unwrap();
        let s = mv.status();
        assert_eq!(s.cells.len(), 1);
        assert!(s.cells[0].source.is_none(), "cell 0 must be the program return");
        mv.stop();
    }

    #[tokio::test]
    async fn tiles_are_relaid_out_as_sources_come_and_go() {
        init();
        let h = handle();
        let mut mv = Multiview::build(&h, default_shape(&h), &fake_proxy("pv2")).unwrap();
        for i in 1..=3 {
            mv.add_tile(Some(format!("cam{i}")), &fake_proxy(&format!("c{i}"))).unwrap();
        }
        // Program plus three cameras is four tiles, so a 2x2 grid.
        let s = mv.status();
        assert_eq!(s.cells.len(), 4);
        assert_eq!((s.cols, s.rows), (2, 2));
        let mut origins: Vec<_> = s.cells.iter().map(|c| (c.x, c.y)).collect();
        origins.sort();
        origins.dedup();
        assert_eq!(origins.len(), 4, "cells overlap");

        mv.remove_tile(&"cam2".to_string()).unwrap();
        let s = mv.status();
        assert_eq!(s.cells.len(), 3);
        assert!(!s.cells.iter().any(|c| c.source.as_deref() == Some("cam2")));
        assert!(s.cells.iter().any(|c| c.source.as_deref() == Some("cam1")));
        assert!(s.cells.iter().any(|c| c.source.as_deref() == Some("cam3")));

        // Removing something that was never there is not an error.
        mv.remove_tile(&"nope".to_string()).unwrap();
        assert_eq!(mv.status().cells.len(), 3);
        mv.stop();
    }

    #[tokio::test]
    async fn every_cell_stays_inside_the_mosaic_at_any_source_count() {
        init();
        let h = handle();
        let shape = default_shape(&h);
        let mut mv = Multiview::build(&h, shape, &fake_proxy("pv3")).unwrap();
        for i in 1..=8 {
            mv.add_tile(Some(format!("cam{i}")), &fake_proxy(&format!("d{i}"))).unwrap();
            for c in mv.status().cells {
                assert!(c.x + c.w <= shape.width, "cell {} overflows width", c.index);
                assert!(c.y + c.h <= shape.height, "cell {} overflows height", c.index);
            }
        }
        mv.stop();
    }

    #[tokio::test]
    async fn a_mosaic_with_no_viewers_still_encodes_without_error() {
        init();
        // The appsink callback sends into a broadcast channel that may have no
        // receivers. That must not be treated as a failure, or the pipeline
        // would tear itself down whenever the operator closed the browser.
        let h = handle();
        let mv = Multiview::build(&h, default_shape(&h), &fake_proxy("pv4")).unwrap();
        assert_eq!(h.shared.frames.receiver_count(), 0);
        let sub = h.subscribe(MultiviewRequest::configured());
        assert_eq!(h.shared.frames.receiver_count(), 1);
        assert_eq!(h.subscribers(), 1);
        drop(sub);
        assert_eq!(h.shared.frames.receiver_count(), 0);
        mv.stop();
    }

    /// Nothing is composited for a preview until somebody asks, and asking is
    /// what builds it.
    #[tokio::test]
    async fn the_preview_is_built_by_asking_and_goes_when_the_asking_stops() {
        init();
        let h = handle();
        let canvas = CanvasCaps::new(&Default::default());
        assert!(h.preview_wanted(&canvas).is_none(), "nobody has asked for a preview");
        let mut mv = Multiview::build(&h, default_shape(&h), &fake_proxy("pvp1")).unwrap();
        assert!(mv.preview_shape().is_none(), "a mosaic must not build a preview by itself");
        let tiles = mv.status().cells.len();

        let sub = h.subscribe_preview(PreviewRequest { fps: 8, width: 320, full: false });
        let shape = h.preview_wanted(&canvas).expect("a subscriber wants one");
        assert!(!shape.full);
        mv.preview_on(shape, h.preview_publisher()).unwrap();
        assert_eq!(mv.preview_shape(), Some(shape));
        assert_eq!(
            mv.status().cells.len(),
            tiles + 1,
            "the preview takes a cell of its own on the mosaic"
        );
        assert!(
            mv.status().cells.iter().any(|c| c.source.as_deref() == Some(PREVIEW_CELL)),
            "and it is named so a UI can label it"
        );

        drop(sub);
        assert!(h.preview_wanted(&canvas).is_none());
        mv.preview_off();
        assert!(mv.preview_shape().is_none());
        assert_eq!(mv.status().cells.len(), tiles, "the preview's cell went with it");
        mv.stop();
    }

    /// The armed scene is drawn from the tiles that are already there, and
    /// disarming takes every slot back.
    #[tokio::test]
    async fn the_armed_scene_is_drawn_from_the_tiles_the_mosaic_already_has() {
        init();
        let h = handle();
        let canvas = CanvasCaps::new(&Default::default());
        let mut mv = Multiview::build(&h, default_shape(&h), &fake_proxy("pvp2")).unwrap();
        for i in 1..=3 {
            mv.add_tile(Some(format!("cam{i}")), &fake_proxy(&format!("pc{i}"))).unwrap();
        }
        let _sub = h.subscribe_preview(PreviewRequest::default());
        let shape = h.preview_wanted(&canvas).expect("a subscriber wants one");
        mv.preview_on(shape, h.preview_publisher()).unwrap();

        let cell = |source: &str, x: i32| preview::Cell {
            source: source.into(),
            x,
            y: 0,
            width: 960,
            height: 540,
            alpha: 1.0,
        };
        mv.apply_preview(&canvas, &[cell("cam1", 0), cell("cam2", 960)]).unwrap();
        assert_eq!(mv.preview_drawn(), 2);

        // A source the mosaic does not carry is skipped rather than drawn as a
        // black rectangle over the ones that are there.
        mv.apply_preview(&canvas, &[cell("cam1", 0), cell("cam9", 960)]).unwrap();
        assert_eq!(mv.preview_drawn(), 1, "a source with no tile has no preview slot");

        // Disarming gives every slot back and leaves the compositor standing,
        // because the client is still watching an empty preview.
        mv.apply_preview(&canvas, &[]).unwrap();
        assert_eq!(mv.preview_drawn(), 0);
        assert!(mv.preview_shape().is_some());
        mv.stop();
    }

    /// A tile going away takes its preview slot with it. Without this the
    /// tile branch would go to NULL under a linked pad.
    #[tokio::test]
    async fn a_source_leaving_takes_its_preview_slot_with_it() {
        init();
        let h = handle();
        let canvas = CanvasCaps::new(&Default::default());
        let mut mv = Multiview::build(&h, default_shape(&h), &fake_proxy("pvp3")).unwrap();
        mv.add_tile(Some("cam1".into()), &fake_proxy("pd1")).unwrap();
        let _sub = h.subscribe_preview(PreviewRequest::default());
        mv.preview_on(h.preview_wanted(&canvas).unwrap(), h.preview_publisher()).unwrap();
        mv.apply_preview(
            &canvas,
            &[preview::Cell {
                source: "cam1".into(),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                alpha: 1.0,
            }],
        )
        .unwrap();
        assert_eq!(mv.preview_drawn(), 1);
        mv.remove_tile(&"cam1".to_string()).unwrap();
        assert_eq!(mv.preview_drawn(), 0, "the slot must go with the tile");
        mv.stop();
    }

    /// `ext.preview = "full"` is the canvas's own size, and one client asking
    /// for it decides for everybody watching.
    #[tokio::test]
    async fn a_full_preview_wins_over_a_mosaic_sized_one() {
        init();
        let h = handle();
        let canvas = CanvasCaps::new(&Default::default());
        let _small = h.subscribe_preview(PreviewRequest { fps: 4, width: 320, full: false });
        let shape = h.preview_wanted(&canvas).expect("a subscriber");
        assert!(!shape.full);
        let full = h.subscribe_preview(PreviewRequest { fps: 8, width: 0, full: true });
        let shape = h.preview_wanted(&canvas).expect("a subscriber");
        assert!(shape.full, "one client asking for full decides");
        assert_eq!((shape.width, shape.height), (canvas.width, canvas.height));
        assert_eq!(shape.fps, 8, "and the highest rate anybody asked for");
        drop(full);
        assert!(!h.preview_wanted(&canvas).expect("still one").full, "and it goes back after");
    }

    #[tokio::test]
    async fn dropping_the_last_pipeline_leaves_none_alive() {
        init();
        let h = handle();
        assert_eq!(h.live_pipelines(), 0);
        let mv = Multiview::build(&h, default_shape(&h), &fake_proxy("pv5")).unwrap();
        assert_eq!(h.live_pipelines(), 1);
        drop(mv);
        assert_eq!(h.live_pipelines(), 0, "a dropped mosaic must not stay alive");
    }

    /// The heart of "nothing runs unless asked": the demand sink sees a build
    /// when the first client arrives and a teardown a linger after the last
    /// one leaves, and nothing at all in between.
    #[tokio::test(start_paused = true)]
    async fn subscribers_build_the_mosaic_and_leaving_tears_it_down() {
        let seen: Arc<Mutex<Vec<Demand>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = {
            let seen = seen.clone();
            Arc::new(move |d: DemandAt| seen.lock().push(d.demand)) as DemandSink
        };
        let h = MultiviewHandle::new(
            MultiviewConfig { linger_secs: 2, ..Default::default() },
            tokio::runtime::Handle::current(),
            sink,
        );
        assert!(!h.wants_thumbs(), "no thumbnail ends before anybody asks");

        let a = h.subscribe(MultiviewRequest::configured());
        assert!(h.wants_thumbs());
        assert_eq!(seen.lock().len(), 1, "the first subscriber builds it");
        assert!(matches!(seen.lock()[0], Demand::Build(_)));
        h.mark_built(h.wanted());
        assert!(h.is_built());

        // A second subscriber at the same shape costs nothing.
        let b = h.subscribe(MultiviewRequest::configured());
        assert_eq!(seen.lock().len(), 1, "a second client must not rebuild");

        // A wider one rebuilds, and the mosaic follows the highest request.
        let c = h.subscribe(MultiviewRequest { fps: 12, width: 1280 });
        assert_eq!(h.wanted().unwrap(), MultiviewShape { fps: 12, width: 1280, height: 720 });
        assert!(matches!(seen.lock()[1], Demand::Build(s) if s.width == 1280));
        h.mark_built(h.wanted());

        // Dropping the wide one settles back to the configured size.
        drop(c);
        assert_eq!(h.wanted().unwrap().width, 960);
        h.mark_built(h.wanted());
        drop(a);
        drop(b);
        assert_eq!(h.subscribers(), 0);

        // The linger has to pass before the teardown.
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        assert!(
            !seen.lock().iter().any(|d| matches!(d, Demand::Teardown)),
            "torn down before the linger elapsed"
        );
        tokio::time::sleep(Duration::from_millis(1_000)).await;
        assert!(
            seen.lock().iter().any(|d| matches!(d, Demand::Teardown)),
            "never torn down"
        );
    }

    /// A client that reconnects inside the linger must not pay for a rebuild.
    #[tokio::test(start_paused = true)]
    async fn a_reconnection_inside_the_linger_keeps_the_mosaic() {
        let seen: Arc<Mutex<Vec<Demand>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = {
            let seen = seen.clone();
            Arc::new(move |d: DemandAt| seen.lock().push(d.demand)) as DemandSink
        };
        let h = MultiviewHandle::new(
            MultiviewConfig { linger_secs: 2, ..Default::default() },
            tokio::runtime::Handle::current(),
            sink,
        );
        let a = h.subscribe(MultiviewRequest::configured());
        h.mark_built(h.wanted());
        drop(a);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let b = h.subscribe(MultiviewRequest::configured());
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(
            !seen.lock().iter().any(|d| matches!(d, Demand::Teardown)),
            "the mosaic was torn down under a client that came back"
        );
        assert_eq!(seen.lock().len(), 1, "and it was never rebuilt either");
        drop(b);
    }

    /// With the switch off nothing is ever asked for, whoever subscribes.
    #[tokio::test(start_paused = true)]
    async fn a_disabled_multiview_asks_for_nothing() {
        let seen: Arc<Mutex<Vec<Demand>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = {
            let seen = seen.clone();
            Arc::new(move |d: DemandAt| seen.lock().push(d.demand)) as DemandSink
        };
        let h = MultiviewHandle::new(
            MultiviewConfig { enabled: false, ..Default::default() },
            tokio::runtime::Handle::current(),
            sink,
        );
        let sub = h.subscribe(MultiviewRequest { fps: 30, width: 1920 });
        assert!(!sub.active());
        assert!(!h.wants_thumbs());
        assert_eq!(h.subscribers(), 0);
        assert!(h.wanted().is_none());
        drop(sub);
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(seen.lock().is_empty(), "a disabled mosaic must ask for nothing");
    }

    /// A mixer small enough to start inside a test, with the mosaic settings
    /// the test is about.
    fn mixer_cfg(multiview: MultiviewConfig) -> crate::config::Config {
        // An empty document is every table's default, which is the shortest
        // way to a valid config that does not have to be rewritten every time
        // somebody adds a field.
        let mut cfg: crate::config::Config = toml::from_str("").unwrap();
        cfg.canvas = crate::config::Canvas {
            width: 320,
            height: 180,
            fps: 15,
            sample_rate: 48000,
            channels: 2,
        };
        cfg.multiview = multiview;
        cfg
    }

    /// The acceptance test for the switch: with `[multiview] enabled = false`
    /// no mosaic pipeline is alive, however hard a client asks.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_mixer_with_multiview_off_never_builds_a_mosaic() {
        init();
        let cfg = mixer_cfg(MultiviewConfig { enabled: false, ..Default::default() });
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg).unwrap();
        mix.start().unwrap();
        let mv = mix.multiview_handle();
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());

        let mut sub = mv.subscribe(MultiviewRequest { fps: 8, width: 1280 });
        assert!(!sub.active());
        // Long enough that a build would have happened if one were coming.
        assert!(
            tokio::time::timeout(Duration::from_millis(600), sub.recv()).await.is_err(),
            "a disabled mosaic must never produce a frame"
        );
        assert_eq!(mv.live_pipelines(), 0, "a mosaic pipeline exists and must not");
        assert!(!mv.is_built());
        assert!(!mv.wants_thumbs(), "no source needs a thumbnail end");
        assert_eq!(handle.status().await.unwrap().multiview.cells.len(), 0);

        drop(sub);
        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
        assert_eq!(mv.live_pipelines(), 0);
    }

    /// And the other half of it: a subscriber gets a mosaic quickly, and the
    /// pipeline goes again a linger after it leaves.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_subscriber_builds_the_mosaic_and_leaving_takes_it_away() {
        init();
        let cfg = mixer_cfg(MultiviewConfig {
            width: 320,
            height: 180,
            linger_secs: 1,
            ..Default::default()
        });
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg).unwrap();
        mix.start().unwrap();
        let mv = mix.multiview_handle();
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
        assert_eq!(mv.live_pipelines(), 0, "a mosaic before anybody asked for one");

        let mut sub = mv.subscribe(MultiviewRequest::configured());
        let frame = next_frame(&mut sub, Duration::from_secs(2)).await;
        assert_eq!(&frame[..2], &[0xFF, 0xD8], "that is not a JPEG");
        assert_eq!(mv.live_pipelines(), 1);
        assert!(mv.is_built());
        assert!(mv.wants_thumbs());
        assert!(mv.fps() > 0.0, "the fps metric never moved");
        // What /metrics will read, in one snapshot.
        let stats = mv.stats();
        assert!(stats.enabled && stats.built);
        assert_eq!(stats.subscribers, 1);
        assert_eq!((stats.width, stats.height), (320, 180));
        assert!(stats.frames > 0 && stats.fps > 0.0);

        drop(sub);
        assert_eq!(mv.subscribers(), 0);
        // Still up during the linger.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(mv.live_pipelines(), 1, "torn down before the linger elapsed");
        // And gone after it.
        for _ in 0..40 {
            if mv.live_pipelines() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(mv.live_pipelines(), 0, "the mosaic outlived its last subscriber");
        assert!(!mv.is_built());
        // And the metrics go to zero rather than keeping the last value.
        let stats = mv.stats();
        assert_eq!((stats.subscribers, stats.fps, stats.built), (0, 0.0, false));

        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    /// The mosaic must arrive at the rate it was asked for. It did not: built
    /// long after the programme, on its own base time, the compositor judged
    /// the proxied tiles against a timeline that had just begun and ran fast
    /// to catch up, so an 8 fps mosaic came out at 16. This counts frames.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_mosaic_runs_at_the_rate_it_was_asked_for() {
        init();
        let cfg = mixer_cfg(MultiviewConfig {
            width: 320,
            height: 180,
            fps: 8,
            linger_secs: 1,
            ..Default::default()
        });
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg).unwrap();
        mix.start().unwrap();
        let mv = mix.multiview_handle();
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());

        // Two seconds of programme before anybody asks, which is what makes
        // this worth testing: the tiles then carry a running time the mosaic
        // has never seen, and an aggregator starting at zero would cover the
        // gap in one burst.
        tokio::time::sleep(Duration::from_secs(2)).await;
        let mut sub = mv.subscribe(MultiviewRequest::configured());
        next_frame(&mut sub, Duration::from_secs(3)).await;
        let started = Instant::now();
        let mut frames = 0u32;
        let mut instant = 0u32;
        while started.elapsed() < Duration::from_secs(2) {
            let before = Instant::now();
            if tokio::time::timeout(Duration::from_millis(500), sub.recv()).await.is_ok() {
                frames += 1;
                if before.elapsed() < Duration::from_millis(10) {
                    instant += 1;
                }
            }
        }
        let rate = frames as f64 / started.elapsed().as_secs_f64();
        assert!(
            (rate - 8.0).abs() < 2.0,
            "asked for 8 fps and got {rate:.1} ({frames} frames)"
        );
        let burst = 4.0 * crate::plugin::harness::timing_slack();
        assert!(
            f64::from(instant) < burst,
            "{instant} frames arrived back to back: the mosaic burst"
        );
        let reported = mv.fps();
        assert!(
            (reported - rate).abs() < 2.0,
            "the metric says {reported:.1} fps and the frames say {rate:.1}"
        );

        drop(sub);
        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    /// Codex's finding: teardown was decided from a generation and an
    /// emptiness check taken separately from the enqueueing, so a subscriber
    /// arriving between the two saw the pipeline, enqueued nothing, and then
    /// lost it.
    ///
    /// The demand carries the generation it was decided at now, and the mixer
    /// thread refuses one that is no longer current.
    #[tokio::test(start_paused = true)]
    async fn a_subscriber_arriving_after_a_teardown_was_decided_keeps_the_mosaic() {
        let seen: Arc<Mutex<Vec<DemandAt>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = {
            let seen = seen.clone();
            Arc::new(move |d: DemandAt| seen.lock().push(d)) as DemandSink
        };
        let h = MultiviewHandle::new(
            MultiviewConfig { linger_secs: 1, ..Default::default() },
            tokio::runtime::Handle::current(),
            sink,
        );
        let a = h.subscribe(MultiviewRequest::configured());
        h.mark_built(h.wanted());
        drop(a);
        // Past the linger, so the teardown has been decided and posted.
        tokio::time::sleep(Duration::from_millis(1_200)).await;
        let teardown = *seen
            .lock()
            .iter()
            .find(|d| matches!(d.demand, Demand::Teardown))
            .expect("no teardown was ever asked for");
        assert!(h.accepts(teardown), "the teardown is current until somebody arrives");

        // The mixer thread has not read it yet, and a client turns up.
        let _b = h.subscribe(MultiviewRequest::configured());
        assert!(
            !h.accepts(teardown),
            "a teardown decided before this subscriber must be refused"
        );
        assert!(h.wanted().is_some(), "and the mosaic is still wanted");
    }

    /// A source with a thumbnail end, for the two tests below.
    fn test_source() -> crate::config::SourceConfig {
        crate::config::SourceConfig::bare("cam1", "test://smpte")
    }

    // --- forensics for a source that stopped ----------------------------
    //
    // A stall that only happens on a hosted runner has to explain itself in
    // the log it fails in, because nobody can reproduce it by hand. What
    // follows is printed by the failing assertion below and by nothing else.

    /// Where the Graphviz dumps go.
    ///
    /// `GST_DEBUG_DUMP_DOT_DIR` when the runner set one, otherwise a directory
    /// beside the test binary, which is `target/<profile>/deps`, so the dumps
    /// land inside the target directory CI already has in hand.
    ///
    /// The variable is deliberately not set from here. GStreamer reads it once
    /// in `gst_init` and remembers it, so a test setting it afterwards changes
    /// nothing, and `setenv` in a test binary running six hundred tests across
    /// as many threads as the machine has is a real way to crash a run. The
    /// dumps below are written by hand instead, which needs no variable at all.
    fn dot_dir() -> std::path::PathBuf {
        if let Some(dir) = std::env::var_os("GST_DEBUG_DUMP_DOT_DIR") {
            if !dir.is_empty() {
                return dir.into();
            }
        }
        std::env::current_exe()
            .ok()
            // the binary, then `deps`, then the profile, then `target`
            .and_then(|exe| exe.ancestors().nth(3).map(|t| t.join("gst-dot")))
            .unwrap_or_else(std::env::temp_dir)
    }

    /// Write each pipeline out as Graphviz and answer where they went.
    fn dump_dot(tag: &str, pipes: &[(String, gst::Pipeline)]) -> String {
        let dir = dot_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return format!("no graphs: {} could not be made ({e})", dir.display());
        }
        let mut written = Vec::new();
        for (name, p) in pipes {
            let path = dir.join(format!("{tag}-{name}.dot"));
            let data = p.debug_to_dot_data(gst::DebugGraphDetails::ALL);
            match std::fs::write(&path, data.as_str()) {
                Ok(()) => written.push(path.display().to_string()),
                Err(e) => written.push(format!("{} ({e})", path.display())),
            }
        }
        format!("graphs: {}", written.join(", "))
    }

    /// One pad: who it is joined to, what it last answered, and every flag
    /// that would explain a branch that stopped moving.
    ///
    /// GStreamer keeps no list of the probes on a pad, so "is a blocking probe
    /// still there" is answered the only way it can be from outside: `blocked`
    /// is a probe holding the pad, `blocking` is one holding it right now.
    fn pad_report(pad: &gst::Pad) -> String {
        let flags = pad.pad_flags();
        let mut notes = Vec::new();
        if !pad.is_active() {
            notes.push("inactive");
        }
        if pad.is_blocked() {
            notes.push("blocked by a probe");
        }
        if pad.is_blocking() {
            notes.push("blocking now");
        }
        if flags.contains(gst::PadFlags::FLUSHING) {
            notes.push("flushing");
        }
        if flags.contains(gst::PadFlags::EOS) {
            notes.push("eos");
        }
        if !pad.is_linked() {
            notes.push("not linked");
        }
        let peer = match pad.peer() {
            Some(p) => {
                let owner = p.parent_element().map(|e| e.name().to_string());
                format!("{}.{}", owner.unwrap_or_else(|| "?".into()), p.name())
            }
            None => "nothing".to_string(),
        };
        format!(
            "{} -> {peer}, last flow {:?}{}",
            pad.name(),
            pad.last_flow_result(),
            if notes.is_empty() { String::new() } else { format!(", {}", notes.join(", ")) }
        )
    }

    /// One element, its pads, its queue fill if it is a queue, and the
    /// children of a bin, because a `proxysrc` holds the queue that fills when
    /// the mosaic stops reading.
    fn element_report(el: &gst::Element, indent: &str, out: &mut String) {
        let (ret, current, pending) = el.state(gst::ClockTime::from_mseconds(200));
        let factory = el.factory().map(|f| f.name().to_string()).unwrap_or_default();
        out.push_str(&format!("{indent}{} ({factory}) {current:?}", el.name()));
        if pending != gst::State::VoidPending {
            out.push_str(&format!(" going to {pending:?}"));
        }
        if ret.is_err() {
            out.push_str(" (the state could not be read)");
        }
        if factory.starts_with("queue") {
            let buffers = el.property::<u32>("current-level-buffers");
            let time = el.property::<u64>("current-level-time");
            let leaky = el.property_value("leaky").serialize().map(|s| s.to_string());
            out.push_str(&format!(
                ", {buffers} buffers, {:.3}s, leaky {}",
                time as f64 / 1e9,
                leaky.unwrap_or_else(|_| "?".into())
            ));
        }
        out.push('\n');
        for pad in el.pads() {
            out.push_str(&format!("{indent}    {}\n", pad_report(&pad)));
        }
        if let Some(bin) = el.downcast_ref::<gst::Bin>() {
            for child in bin.children() {
                element_report(&child, &format!("{indent}    "), out);
            }
        }
    }

    /// A whole pipeline, element by element.
    fn pipeline_report(name: &str, p: &gst::Pipeline) -> String {
        let (ret, current, pending) = p.state(gst::ClockTime::from_mseconds(200));
        let mut out = format!("{name}: {current:?}");
        if pending != gst::State::VoidPending {
            out.push_str(&format!(" going to {pending:?}"));
        }
        if ret.is_err() {
            out.push_str(" (the state could not be read)");
        }
        out.push('\n');
        for el in p.children().iter().rev() {
            element_report(el, "  ", &mut out);
        }
        out
    }

    /// Everything the next failure should carry: the source's own pipeline,
    /// the programme, the mosaic if one is up, and a graph of each on disk.
    fn forensics(round: usize, pipes: &[(String, gst::Pipeline)]) -> String {
        let mut all: Vec<(String, gst::Pipeline)> = pipes.to_vec();
        // The mosaic comes and goes, so it is looked up when it is wanted. The
        // registry answers with whichever mixer registered last, which in a
        // test binary running several at once may not be this one: it is here
        // because a mosaic that is up at all is worth seeing, and the graph
        // says which pipeline it is.
        if let Some(mv) = crate::observe::introspect::pipeline("multiview") {
            all.push(("multiview".to_string(), mv));
        }
        let mut out = format!("\n--- what the pipelines looked like on round {round} ---\n");
        for (name, p) in &all {
            out.push_str(&pipeline_report(name, p));
        }
        out.push_str(&dump_dot(&format!("stalled-round-{round}"), &all));
        out.push('\n');
        out
    }

    /// Reported from live runs: a `test://` source went live and then stalled
    /// within about twenty seconds, repeatedly, whenever a client subscribed to
    /// the mosaic and left again.
    ///
    /// The mosaic stops reading its `proxysrc`s while it is being torn down or
    /// rebuilt. The thumbnail branch hanging off the source's `vtee` then fills
    /// up, and a queue that blocks when full holds the tee, which holds the
    /// programme branch beside it, which is where the liveness probe lives. The
    /// source looked dead and the supervisor restarted it. This subscribes and
    /// leaves repeatedly and insists the source stays live throughout.
    ///
    /// What "stays live" means here needs saying, because the obvious reading
    /// of it is not testable on a shared machine. The liveness probe sits on
    /// the programme `proxysink`, and `stall_timeout_secs` is two seconds, so
    /// the source reads `Stalled` whenever two seconds of frames fail to reach
    /// the mixer for any reason at all. On a hosted runner with two shared
    /// cores, software Mesa and forty other pipelines in the same test binary,
    /// that happens to a healthy `videotestsrc` now and again: this failed on
    /// round 8 of 12 on one Linux runner and round 10 of 12 on another, while
    /// passing every round on the same commit on macOS and on the developer's
    /// machine. A single reading is therefore not evidence of the defect.
    ///
    /// Two readings in a row are. The defect holds the source's tee for as long
    /// as the mosaic keeps coming and going, so the round after a stalled round
    /// is stalled too, and the idle time climbs instead of returning to one
    /// frame. A runner that did not schedule a thread for a moment gives one
    /// stalled round and a healthy one behind it, because the next frame to
    /// arrive puts the idle time back to about sixty milliseconds. So the
    /// assertion is on the second reading, the idle time of every round is kept
    /// for the failure message, and what used to be a bare "stalled on round 8"
    /// now prints the series that led to it.
    ///
    /// A failure carries its own evidence now, because this one only happens
    /// on a hosted Linux runner and nobody can reproduce it by hand: the
    /// series, then every element of the source's pipeline and of the
    /// programme with its state, its pads, what each pad last answered a push
    /// with, whether a probe still holds it, the fill of every queue, and a
    /// Graphviz graph of each pipeline on disk with the assertion saying where
    /// it went. A source killed by a flushing return reads `last flow
    /// Err(Flushing)` on the pads between the tee and whatever swallowed it.
    ///
    /// Widening `stall_timeout_secs` by `timing_slack()` was the other way to
    /// do this and is not what happens here. It would do nothing for the
    /// `linux, no GPU, software codecs` job, which is hosted and does not set
    /// `GODWINMIX_TIMING_SLACK`, and a timeout wider than the loop is long
    /// would mean a source held for the whole run never reads `Stalled` at all.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_subscriber_coming_and_going_never_stalls_a_source() {
        init();
        let mut cfg = mixer_cfg(MultiviewConfig {
            width: 320,
            height: 180,
            fps: 8,
            linger_secs: 0,
            ..Default::default()
        });
        cfg.sources = vec![test_source()];
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg).unwrap();
        mix.start().unwrap();
        let mv = mix.multiview_handle();
        // Taken from the mixer while it is still in hand, because a failure
        // below has to say what this source's own pipeline looked like and the
        // mixer goes off to its thread on the next line. A pipeline is a
        // reference: holding one costs nothing and keeps nothing alive past
        // the shutdown at the end.
        let pipes: Vec<(String, gst::Pipeline)> = [
            ("input-cam1".to_string(), mix.source_pipeline("cam1")),
            ("programme".to_string(), Some(mix.program_pipeline().clone())),
        ]
        .into_iter()
        .filter_map(|(name, p)| p.map(|p| (name, p)))
        .collect();
        println!("graphs from this test go to {}", dot_dir().display());
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());

        // Let the source deliver a picture before anything is asked of it.
        let live = async {
            loop {
                let status = handle.status().await.unwrap();
                if status.sources.iter().any(|s| s.state == crate::state::SourceState::Live) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), live)
            .await
            .expect("the source never went live");

        /// One subscribe and leave, and what the source looked like afterwards.
        ///
        /// Answers `(state, video_idle_ms)`. The status read has a deadline of
        /// its own, so a mixer thread that has wedged fails the test here
        /// rather than hanging the suite.
        async fn churn(
            mv: &MultiviewHandle,
            handle: &crate::mixer::MixerHandle,
            round: usize,
            pipes: &[(String, gst::Pipeline)],
        ) -> (Option<crate::state::SourceState>, Option<u64>) {
            // Both deadlines are wall clock, so a machine that has declared
            // itself slow gets more of it. Neither is what is being measured:
            // the first is how long a mosaic frame may take to arrive and the
            // second is the answer to "is the mixer thread still there".
            let budget = Duration::from_secs(2).mul_f64(crate::plugin::harness::timing_slack());
            let mut sub = mv.subscribe(MultiviewRequest::configured());
            let _ = tokio::time::timeout(budget, sub.recv()).await;
            drop(sub);
            tokio::time::sleep(Duration::from_millis(120)).await;
            let status = tokio::time::timeout(budget, handle.status())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "the mixer stopped answering on round {round}{}",
                        forensics(round, pipes)
                    )
                })
                .unwrap();
            let source = status.sources.first();
            (source.map(|s| s.state), source.and_then(|s| s.video_idle_ms))
        }

        let stalled = Some(crate::state::SourceState::Stalled);
        // `(round, state, video_idle_ms)` for every round, printed if this
        // fails: a held tee shows the idle time climbing round after round, a
        // runner that starved a thread shows one spike and a recovery.
        let mut seen: Vec<(usize, Option<crate::state::SourceState>, Option<u64>)> = Vec::new();
        let mut was_stalled = false;
        for round in 0..12 {
            let (state, idle) = churn(&mv, &handle, round, &pipes).await;
            seen.push((round, state, idle));
            assert!(
                !(was_stalled && state == stalled),
                "the source was judged stalled on rounds {} and {round}, one after the other, \
                 by a mosaic coming and going: {seen:?}{}",
                round - 1,
                forensics(round, &pipes)
            );
            was_stalled = state == stalled;
        }
        // A stall on the last round has no round behind it to confirm it, so
        // the mosaic is made to come and go once more rather than letting the
        // one reading the loop ends on go unjudged.
        if was_stalled {
            let (state, idle) = churn(&mv, &handle, 12, &pipes).await;
            seen.push((12, state, idle));
            assert!(
                state != stalled,
                "the source was judged stalled on the last round and on the one after it, \
                 by a mosaic coming and going: {seen:?}{}",
                forensics(12, &pipes)
            );
        }

        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    /// Reported from live runs: the core stopped answering HTTP entirely right
    /// after "last multiview subscriber left, taking the mosaic down", stayed
    /// alive and never recovered.
    ///
    /// Anything that wedges the mixer thread does that, because every control
    /// call ends at `handle.status()`. This builds and tears the mosaic down
    /// fifty times while a source runs and another task polls status, with a
    /// deadline on every step, so a teardown that blocks fails here instead of
    /// in a show.
    #[tokio::test(flavor = "multi_thread")]
    async fn fifty_mosaic_teardowns_never_wedge_the_mixer() {
        init();
        let mut cfg = mixer_cfg(MultiviewConfig {
            width: 320,
            height: 180,
            fps: 8,
            linger_secs: 0,
            ..Default::default()
        });
        cfg.sources = vec![test_source()];
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg).unwrap();
        mix.start().unwrap();
        let mv = mix.multiview_handle();
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());

        // A second task asking the mixer questions throughout. It records the
        // worst answer time it saw, which is the number that matters: a mixer
        // thread held for two seconds is a mixer thread that is not switching
        // cameras either.
        let polling = handle.clone();
        let worst = Arc::new(AtomicU64::new(0));
        let poll_worst = worst.clone();
        let poller = tokio::spawn(async move {
            for _ in 0..300 {
                let at = Instant::now();
                if tokio::time::timeout(Duration::from_secs(3), polling.status()).await.is_err() {
                    return Err("the mixer stopped answering status");
                }
                let ms = at.elapsed().as_millis() as u64;
                poll_worst.fetch_max(ms, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok(())
        });

        for round in 0..50 {
            let sub = mv.subscribe(MultiviewRequest::configured());
            tokio::time::sleep(Duration::from_millis(30)).await;
            drop(sub);
            tokio::time::sleep(Duration::from_millis(30)).await;
            let alive = tokio::time::timeout(Duration::from_secs(3), handle.status()).await;
            assert!(alive.is_ok(), "the mixer wedged on teardown round {round}");
        }

        poller.abort();
        let worst_ms = worst.load(Ordering::Relaxed);
        assert!(
            worst_ms < 1_500,
            "a status call took {worst_ms} ms: the mixer thread is being held across a teardown"
        );

        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    /// The same, for the preview compositor.
    ///
    /// `fifty_mosaic_teardowns_never_wedge_the_mixer` covers a whole pipeline
    /// coming and going. The preview is the other shape of the problem: it is
    /// built into a pipeline that is already running, fed off the tile tees,
    /// and taken down again with everything around it still pushing. The
    /// Linux smoke wedged exactly there, on a run where the preview had not
    /// produced its first frame before the client that asked for it left.
    ///
    /// So this asks for a preview, gives it up before a frame can arrive, and
    /// insists the mixer is still answering. The scene is pushed first, so
    /// each round binds real slots on the compositor and gives them back.
    #[tokio::test(flavor = "multi_thread")]
    async fn forty_preview_teardowns_never_wedge_the_mixer() {
        init();
        let mut cfg = mixer_cfg(MultiviewConfig {
            width: 320,
            height: 180,
            fps: 8,
            linger_secs: 0,
            ..Default::default()
        });
        cfg.sources = vec![test_source()];
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg).unwrap();
        mix.start().unwrap();
        let mv = mix.multiview_handle();
        let preview = mix.preview_handle();
        let canvas = mix.canvas().clone();
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
        preview.set_scene(vec![preview::Cell {
            source: "cam1".into(),
            x: 0,
            y: 0,
            width: canvas.width,
            height: canvas.height,
            alpha: 1.0,
        }]);

        for round in 0..40u64 {
            let sub = mv.subscribe_preview(PreviewRequest { fps: 8, width: 320, full: false });
            // Deliberately shorter than a first frame takes on a slow runner:
            // the teardown that wedged the mixer was the one that landed
            // before the compositor had produced anything.
            tokio::time::sleep(Duration::from_millis((round % 4) * 40)).await;
            drop(sub);
            tokio::time::sleep(Duration::from_millis(20)).await;
            let alive = tokio::time::timeout(Duration::from_secs(5), handle.status()).await;
            assert!(
                matches!(alive, Ok(Ok(_))),
                "the mixer stopped answering after preview teardown round {round}: {:?}",
                alive.map(|r| r.map(|_| ()))
            );
        }

        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    /// Resizing must keep real programme pixels, not merely send black JPEGs.
    #[tokio::test(flavor = "multi_thread")]
    async fn programme_return_survives_mosaic_resizes() {
        init();
        let mut cfg = mixer_cfg(MultiviewConfig {
            width: 320, height: 180, fps: 8, linger_secs: 0,
            ..Default::default()
        });
        cfg.sources = vec![test_source()];
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg).unwrap();
        mix.start().unwrap();
        let mv = mix.multiview_handle();
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
        handle.send(crate::mixer::Command::Take {
            source: Some("cam1".into()), at_running_time_ms: None, ack: None,
        }).unwrap();
        let mut held = None;
        for width in [320, 640, 960, 320] {
            let mut next = mv.subscribe(MultiviewRequest { fps: 8, width });
            drop(held.take());
            let visible = async {
                loop {
                    let bytes = match next.recv().await {
                        Ok(bytes) => bytes,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(err) => panic!("the mosaic subscription closed: {err}"),
                    };
                    let img = crate::snapshot::decode_jpeg(&bytes).unwrap();
                    if img.width() != width as u32 { continue; }
                    // The programme is cell zero, at the left of a two cell row.
                    let colourful = img.enumerate_pixels().filter(|(x, _, p)| {
                        *x < img.width() / 2 && p.0.iter().max().unwrap() - p.0.iter().min().unwrap() > 80
                    }).count();
                    if colourful > (img.width() * img.height() / 10) as usize { break; }
                }
            };
            let result = tokio::time::timeout(Duration::from_secs(6), visible).await;
            if result.is_err() {
                handle.send(crate::mixer::Command::Shutdown).unwrap();
                tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
                panic!("programme return stayed black at width {width}");
            }
            held = Some(next);
        }
        drop(held);
        handle.send(crate::mixer::Command::Shutdown).unwrap();
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_request_is_clamped_and_keeps_the_configured_aspect() {
        let h = handle();
        let s = h.shared.shape_of(MultiviewRequest { fps: 240, width: 4096 });
        assert_eq!((s.width, s.height, s.fps), (1920, 1080, MAX_MOSAIC_FPS));
        let s = h.shared.shape_of(MultiviewRequest { fps: -1, width: 1 });
        assert_eq!((s.width, s.height), (MIN_MOSAIC_WIDTH, 90));
        assert_eq!(s.fps, MultiviewConfig::default().fps);
        // Odd widths would break the 4:2:0 chroma the encoder wants.
        let s = h.shared.shape_of(MultiviewRequest { fps: 0, width: 641 });
        assert_eq!(s.width % 2, 0);
        assert_eq!(s.height % 2, 0);
    }
}
