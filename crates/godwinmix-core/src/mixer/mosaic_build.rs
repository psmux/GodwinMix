//! The mosaic built off the mixer loop.
//!
//! Building the mosaic makes a pipeline, a tile per source (a thumbnail end
//! attached to the running source and a branch in the mosaic), the preview
//! compositor when one is asked for, and starts it all. None of it is the
//! programme's. On a quiet machine it took a few hundred milliseconds; in
//! the seconds after a restart, with every source starting at once, the web
//! UI's first ask held the loop for two and a half seconds and every Take
//! behind it waited.
//!
//! So the loop plans the build and a blocking worker does it, the way the
//! preview's filters are built (`preview_draw`):
//!
//! ```text
//!   loop:   wanted shape, no mosaic -> plan (sources, preview) -> worker
//!   worker: tear down the old mosaic, if any -> build, tiles, preview, start
//!           -> Command::MosaicBuilt -> loop
//!   loop:   still wanted at this shape? no  -> worker tears it down
//!                                       yes -> tiles for sources added or
//!                                              removed meanwhile, the
//!                                              programme return, the preview
//! ```
//!
//! One job is out at a time, so a teardown and a build never touch the same
//! source's thumbnail end at once. Demands that land while one is out change
//! nothing; the loop looks at what is wanted when the job comes back. The
//! programme return branch is the one piece on the programme's pipeline, and
//! it is attached on the loop once the mosaic is running, as before.

use super::{clip_end, Command, Mixer, TimelineAligner};
use crate::caps::CanvasCaps;
use crate::gstutil::{self, BusEvent};
use crate::input::InputPipeline;
use crate::multiview::{preview::PreviewShape, Multiview, MultiviewHandle, MultiviewShape};
use anyhow::{Context, Result};
use gstreamer as gst;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// What the loop knows about the job that is out.
#[derive(Default)]
pub(super) struct Mosaic {
    /// The shape being built, while a worker builds it. `Some` with no shape
    /// is a teardown on its own.
    out: Option<Option<MultiviewShape>>,
}

/// A source's tile, as planned on the loop.
struct TilePlan {
    input: Arc<InputPipeline>,
    generation: u64,
    aligner: Option<Arc<TimelineAligner>>,
    clip: bool,
}

/// Everything the worker needs, cloned off the loop.
struct Plan {
    shape: MultiviewShape,
    handle: MultiviewHandle,
    program_video: gst::Element,
    programme: gst::Pipeline,
    canvas: CanvasCaps,
    bus_tx: tokio::sync::mpsc::Sender<BusEvent>,
    tiles: Vec<TilePlan>,
    preview: Option<PreviewShape>,
}

/// A mosaic on its way out, with the sources whose thumbnail ends feed it.
pub(super) struct Retired {
    mv: Multiview,
    inputs: Vec<Arc<InputPipeline>>,
}

impl Retired {
    /// Thumbnail ends first, then the pipeline: see `Mixer::drop_mosaic`.
    fn tear_down(self) {
        for input in &self.inputs {
            input.detach_thumb_end();
        }
        drop(self.mv);
    }
}

/// A built and running mosaic, back on the loop.
pub struct Built {
    mv: Multiview,
    /// Each source tile's source, by id and instance.
    tiles: Vec<(Arc<InputPipeline>, u64)>,
}

impl std::fmt::Debug for Built {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Built({:?}, {} tiles)", self.mv.shape(), self.tiles.len())
    }
}

/// The worker's answer: the mosaic, why not, or nothing for a teardown.
pub type Back = Option<std::result::Result<Built, String>>;

impl Mixer {
    /// True while a worker builds or tears down a mosaic.
    pub(super) fn mosaic_out(&self) -> bool {
        self.mosaic.out.is_some()
    }

    /// The shape a worker is building, so a source added meanwhile makes its
    /// thumbnail at the mosaic's rate.
    pub(super) fn mosaic_building(&self) -> Option<MultiviewShape> {
        self.mosaic.out.flatten()
    }

    /// Take the mosaic down off the loop, and build `shape` after it when
    /// asked. Does nothing while another job is out: its return looks again.
    pub(super) fn send_mosaic_job(&mut self, shape: Option<MultiviewShape>) {
        let here = self.multiview.as_ref().map(|mv| mv.shape());
        if !self.mosaic_out() && (here != shape || shape.is_none()) {
            let old = self.retire_mosaic();
            self.mosaic_job(old, shape);
        }
    }

    /// One job: `old` down, then `shape` up, in that order on one worker.
    fn mosaic_job(&mut self, old: Option<Retired>, shape: Option<MultiviewShape>) {
        let plan = shape.map(|s| self.mosaic_plan(s));
        if old.is_none() && plan.is_none() {
            return;
        }
        self.mosaic.out = Some(shape);
        debug!(?shape, "the mosaic goes to a worker");
        let handle = self.handle.clone();
        self.rt.spawn_blocking(move || {
            if let Some(old) = old {
                old.tear_down();
            }
            let back = plan.map(|p| build(p).map_err(|e| format!("{e:#}")));
            // A refused send drops the mosaic, which stops it.
            let _ = handle.send(Command::MosaicBuilt(Box::new(back)));
        });
    }

    /// Unhook the running mosaic from the programme and hand it back to be
    /// taken down off the loop.
    fn retire_mosaic(&mut self) -> Option<Retired> {
        let mv = self.multiview.take()?;
        self.detach_programme_return();
        self.mv.mark_built(None);
        Some(Retired { mv, inputs: self.thumbed(Vec::new()) })
    }

    /// Every source that may hold a thumbnail end for a mosaic: `tiles` and
    /// every source here now, which takes in one added during a build.
    fn thumbed(&self, mut tiles: Vec<Arc<InputPipeline>>) -> Vec<Arc<InputPipeline>> {
        tiles.extend(self.sources.iter().map(|s| s.input.clone()));
        tiles
    }

    fn mosaic_plan(&self, shape: MultiviewShape) -> Plan {
        let tiles = self
            .sources
            .iter()
            .filter(|s| s.input.id != super::AD_ID)
            .map(|s| TilePlan {
                input: s.input.clone(),
                generation: s.generation,
                aligner: s.aligner.clone(),
                clip: s.clip_end.is_some(),
            })
            .collect();
        Plan {
            shape,
            handle: self.mv.clone(),
            program_video: self.pgm_video_proxy.clone(),
            programme: self.program.clone(),
            canvas: self.canvas.clone(),
            bus_tx: self.bus_tx.clone(),
            tiles,
            preview: self.mv.preview_wanted(&self.canvas),
        }
    }

    /// The worker is back. Keep the mosaic if it is still the one wanted,
    /// otherwise send it down again; then look at what is wanted now.
    pub(super) fn mosaic_built(&mut self, back: Back) -> Result<()> {
        self.mosaic.out = None;
        let wanted = self.mv.wanted();
        match back {
            None => self.send_mosaic_job(wanted),
            Some(Err(e)) => {
                // Not asked again: a build that failed once fails the same
                // way, and the next subscriber's demand tries it afresh.
                self.mv.mark_built(None);
                warn!(error = %e, "the multiview could not be built");
            }
            Some(Ok(built)) if wanted == Some(built.mv.shape()) => return self.keep_mosaic(built),
            Some(Ok(built)) => {
                // Built for nobody, or at a shape nobody wants now: down off
                // the loop, ahead of the build that is wanted, if any.
                debug!(?wanted, "a mosaic built for a demand that has moved on goes down off the loop");
                self.mv.mark_built(None);
                let inputs = self.thumbed(built.tiles.into_iter().map(|(i, _)| i).collect());
                self.mosaic_job(Some(Retired { mv: built.mv, inputs }), wanted);
            }
        }
        self.preview_demand_now()?;
        self.broadcast_status();
        Ok(())
    }

    /// Put a built mosaic in place: tiles for what changed while it was
    /// built, the programme return, the preview.
    fn keep_mosaic(&mut self, built: Built) -> Result<()> {
        let Built { mut mv, tiles } = built;
        for (input, generation) in &tiles {
            let here = self.sources.iter().any(|s| Arc::ptr_eq(&s.input, input) && s.generation == *generation);
            if !here {
                mv.remove_tile(&input.id).ok();
                input.detach_thumb_end();
            }
        }
        let shape = mv.shape();
        for slot in self.sources.iter().filter(|s| s.input.id != super::AD_ID) {
            if tiles.iter().any(|(i, _)| Arc::ptr_eq(i, &slot.input)) {
                continue;
            }
            let plan = TilePlan {
                input: slot.input.clone(),
                generation: slot.generation,
                aligner: slot.aligner.clone(),
                clip: slot.clip_end.is_some(),
            };
            add_tile(&mut mv, &plan, &self.canvas, shape).context("adding a tile for a source added meanwhile")?;
        }
        crate::observe::laps::mark("mosaic tiles reconciled");
        self.multiview = Some(mv);
        // Last, once the mosaic is running: see `attach_programme_return`.
        self.attach_programme_return()
            .context("attaching the programme return branch for the mosaic")?;
        crate::observe::laps::mark("programme return attached");
        self.show_held_clips_again();
        info!(?shape, "multiview built for a subscriber");
        // The preview lives in the mosaic's pipeline. The worker built it at
        // what was asked then; this makes it what is asked now and draws it.
        self.preview_demand_now()?;
        self.broadcast_status();
        Ok(())
    }
}

/// Build and start the mosaic. Slow, and meant for a worker thread. A
/// mosaic that fails half way gives back the thumbnail ends it took.
fn build(plan: Plan) -> Result<Built> {
    let mut mv = Multiview::build(&plan.handle, plan.shape, &plan.program_video).context("building multiview")?;
    let mut tiles = Vec::with_capacity(plan.tiles.len());
    match assemble(&plan, &mut mv, &mut tiles) {
        Ok(()) => Ok(Built { mv, tiles }),
        Err(e) => {
            plan.handle.mark_built(None);
            let inputs = plan.tiles.iter().map(|t| t.input.clone()).collect();
            Retired { mv, inputs }.tear_down();
            Err(e)
        }
    }
}

/// The mosaic's bus, tiles and preview, then start it.
fn assemble(plan: &Plan, mv: &mut Multiview, tiles: &mut Vec<(Arc<InputPipeline>, u64)>) -> Result<()> {
    mv.attach_watch(gstutil::watch_bus(mv.pipeline(), gstutil::BusOwner::Multiview, plan.bus_tx.clone())?);
    for t in &plan.tiles {
        add_tile(mv, t, &plan.canvas, plan.shape).context("adding multiview tile")?;
        tiles.push((t.input.clone(), t.generation));
    }
    if let Some(shape) = plan.preview {
        // Marked built on the loop, once this mosaic is kept.
        mv.preview_on(shape, plan.handle.preview_publisher()).context("building the preview compositor")?;
    }
    mv.follow_clock_of(&plan.programme);
    // Marked built before the first frame can leave it: a subscriber that
    // reads `is_built` the moment its frame lands must find it true.
    plan.handle.mark_built(Some(plan.shape));
    mv.start().context("starting multiview")
}

/// One source's tile: a thumbnail end on the running source, a branch in
/// the mosaic fed from it.
fn add_tile(mv: &mut Multiview, t: &TilePlan, canvas: &CanvasCaps, shape: MultiviewShape) -> Result<()> {
    // A source pays for its thumbnail branch only while a mosaic exists to
    // show it; it is taken back at teardown.
    let proxy = t.input.attach_thumb_end(canvas, shape.fps).context("attaching a thumbnail end for the mosaic")?;
    mv.add_tile_with(Some(t.input.id.clone()), &proxy, |pad| {
        if let Some(a) = &t.aligner {
            a.follow_tile(pad);
        }
        if t.clip {
            clip_end::ClipEnd::keep_last_frame(pad);
        }
    })?;
    crate::observe::laps::mark("multiview tile");
    Ok(())
}
