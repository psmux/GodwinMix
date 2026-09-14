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
use tracing::{debug, info};

/// Frames are dropped rather than queued when a viewer cannot keep up. A late
/// preview frame has no value, so the newest always wins.
const FRAME_CHANNEL_DEPTH: usize = 2;

/// Bounds on what a subscriber may ask for. A client asking for a 4K mosaic at
/// 60 fps would cost more than the programme it is previewing.
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
}

type DemandSink = Arc<dyn Fn(Demand) + Send + Sync>;

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
}

impl Shared {
    fn ask(&self, d: Demand) {
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
        Self {
            shared: Arc::new(Shared {
                cfg,
                frames,
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
            // and its own settle has already asked for what it needs.
            if shared.generation.load(Ordering::SeqCst) != gen {
                return;
            }
            if !shared.subs.lock().is_empty() {
                return;
            }
            info!("last multiview subscriber left, taking the mosaic down");
            shared.ask(Demand::Teardown);
        });
    }
}

struct Tile {
    /// None for the program return cell.
    source: Option<SourceId>,
    pad: gst::Pad,
    branch: Vec<gst::Element>,
}

pub struct Multiview {
    cfg: MultiviewConfig,
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
        let vqueue = gstutil::queue_thread("mv-q")?;
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
        let queue = gstutil::queue_thread(&format!("mv-q-{tag}"))?;
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

        let branch = vec![src, queue, rate, scale, caps];
        self.pipeline.add_many(&branch).context("adding tile branch")?;
        gst::Element::link_many(&branch).context("linking tile branch")?;

        let pad = self
            .compositor
            .request_pad_simple("sink_%u")
            .context("compositor refused a tile pad")?;
        // Letterbox rather than stretch, so a 4:3 camera beside a 16:9 one
        // still looks like itself.
        pad.set_property_from_str("sizing-policy", "keep-aspect-ratio");
        branch
            .last()
            .unwrap()
            .static_pad("src")
            .context("tile branch has no src pad")?
            .link(&pad)
            .context("linking tile into the mosaic")?;

        for el in &branch {
            el.sync_state_with_parent().ok();
        }

        self.tiles.push(Tile { source, pad, branch });
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
        for el in &tile.branch {
            let _ = el.set_state(gst::State::Null);
            let _ = self.pipeline.remove(el);
        }
        self.compositor.release_request_pad(&tile.pad);
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
            Arc::new(move |d: Demand| seen.lock().push(d)) as DemandSink
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
            Arc::new(move |d: Demand| seen.lock().push(d)) as DemandSink
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
            Arc::new(move |d: Demand| seen.lock().push(d)) as DemandSink
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
        let frame = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .expect("no mosaic frame within two seconds of subscribing")
            .expect("the frame channel closed");
        assert_eq!(&frame[..2], &[0xFF, 0xD8], "that is not a JPEG");
        assert_eq!(mv.live_pipelines(), 1);
        assert!(mv.is_built());
        assert!(mv.wants_thumbs());
        assert!(mv.fps() > 0.0, "the fps metric never moved");

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
        tokio::time::timeout(Duration::from_secs(3), sub.recv()).await.unwrap().unwrap();
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
        assert!(instant < 4, "{instant} frames arrived back to back: the mosaic burst");
        let reported = mv.fps();
        assert!(
            (reported - rate).abs() < 2.0,
            "the metric says {reported:.1} fps and the frames say {rate:.1}"
        );

        drop(sub);
        let _ = handle.send(crate::mixer::Command::Shutdown);
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
