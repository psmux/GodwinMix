//! The operator's mosaic: every source plus a program return, composited into
//! one picture on the server and pushed to the UI as JPEG frames.
//!
//! Compositing server side is what keeps the control UI usable over a bad
//! remote link. Cost is one encode and one connection no matter how many
//! cameras are attached, instead of one of each per camera. The browser draws
//! a single image and overlays clickable regions on it.
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
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, info};

/// Frames are dropped rather than queued when a viewer cannot keep up. A late
/// preview frame has no value, so the newest always wins.
const FRAME_CHANNEL_DEPTH: usize = 2;

struct Tile {
    /// None for the program return cell.
    source: Option<SourceId>,
    pad: gst::Pad,
    branch: Vec<gst::Element>,
}

pub struct Multiview {
    cfg: MultiviewConfig,
    pipeline: gst::Pipeline,
    compositor: gst::Element,
    tiles: Vec<Tile>,
    grid: Grid,
    frames: broadcast::Sender<Arc<[u8]>>,
}

impl Multiview {
    pub fn build(
        cfg: &MultiviewConfig,
        program_video: &gst::Element,
    ) -> Result<Self> {
        let pipeline = gst::Pipeline::with_name("multiview");
        let fps = gst::Fraction::new(cfg.fps.max(1), 1);

        // force-live and ignore-inactive-pads together make the mosaic tick
        // along on its own schedule even when every camera is dead. Without
        // them a stalled input would freeze the operator's whole view.
        let compositor = gstutil::make_live_aggregator("compositor", "mv-comp")?;
        compositor.set_property_from_str("background", "black");
        crate::probe::set_bool(&compositor, "ignore-inactive-pads", true);

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

        let (frames, _) = broadcast::channel(FRAME_CHANNEL_DEPTH);
        {
            let frames = frames.clone();
            sink.set_callbacks(
                gst_app::AppSinkCallbacks::builder()
                    .new_sample(move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                        let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                        // A send failure just means nobody is watching.
                        let _ = frames.send(Arc::from(map.as_slice()));
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
            cfg: cfg.clone(),
            pipeline,
            compositor,
            tiles: Vec::new(),
            grid: Grid::for_tiles(1, cfg.width, cfg.height),
            frames,
        };

        if cfg.include_program {
            mv.add_tile(None, program_video)?;
        }
        Ok(mv)
    }

    /// Subscribe to the JPEG frame stream. Each WebSocket viewer takes one.
    #[allow(dead_code)]
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<[u8]>> {
        self.frames.subscribe()
    }

    /// The sending half, so the control plane can hand a fresh receiver to
    /// each viewer that connects rather than holding one open forever.
    pub fn sender(&self) -> broadcast::Sender<Arc<[u8]>> {
        self.frames.clone()
    }

    /// Attach one more tile, fed from a `proxysink` in another pipeline.
    pub fn add_tile(&mut self, source: Option<SourceId>, proxy: &gst::Element) -> Result<()> {
        let tag = source.clone().unwrap_or_else(|| "program".into());

        let src = make("proxysrc", &format!("mv-src-{tag}"))?;
        src.set_property("proxysink", proxy);
        let queue = gstutil::queue_thread(&format!("mv-q-{tag}"))?;
        let rate = make("videorate", &format!("mv-rate-{tag}"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        let _ = gst::init();
    }

    /// Stand-in for a proxysink living in another pipeline.
    fn fake_proxy(name: &str) -> gst::Element {
        let p = gst::Pipeline::with_name(&format!("fake-{name}"));
        let sink = make("proxysink", name).unwrap();
        p.add(&sink).unwrap();
        std::mem::forget(p);
        sink
    }

    #[test]
    fn mosaic_starts_with_only_the_program_return() {
        init();
        let mv = Multiview::build(&MultiviewConfig::default(), &fake_proxy("pv")).unwrap();
        let s = mv.status();
        assert_eq!(s.cells.len(), 1);
        assert!(s.cells[0].source.is_none(), "cell 0 must be the program return");
        mv.stop();
    }

    #[test]
    fn tiles_are_relaid_out_as_sources_come_and_go() {
        init();
        let mut mv = Multiview::build(&MultiviewConfig::default(), &fake_proxy("pv2")).unwrap();
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

    #[test]
    fn every_cell_stays_inside_the_mosaic_at_any_source_count() {
        init();
        let cfg = MultiviewConfig::default();
        let mut mv = Multiview::build(&cfg, &fake_proxy("pv3")).unwrap();
        for i in 1..=8 {
            mv.add_tile(Some(format!("cam{i}")), &fake_proxy(&format!("d{i}"))).unwrap();
            for c in mv.status().cells {
                assert!(c.x + c.w <= cfg.width, "cell {} overflows width", c.index);
                assert!(c.y + c.h <= cfg.height, "cell {} overflows height", c.index);
            }
        }
        mv.stop();
    }

    #[test]
    fn a_mosaic_with_no_viewers_still_encodes_without_error() {
        init();
        // The appsink callback sends into a broadcast channel that may have no
        // receivers. That must not be treated as a failure, or the pipeline
        // would tear itself down whenever the operator closed the browser.
        let mv = Multiview::build(&MultiviewConfig::default(), &fake_proxy("pv4")).unwrap();
        assert_eq!(mv.frames.receiver_count(), 0);
        let rx = mv.subscribe();
        assert_eq!(mv.frames.receiver_count(), 1);
        drop(rx);
        assert_eq!(mv.frames.receiver_count(), 0);
        mv.stop();
    }
}
