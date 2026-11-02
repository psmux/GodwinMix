//! The armed scene drawn in the preview, with its filters built off the loop.
//!
//! The mixer loop is the one that serves Take and Cut. Drawing the preview
//! is a handful of pad writes and links, which is cheap, but a new item
//! with a filter also builds that filter: a look reads its LUT file, a
//! cutout starts its worker, a key looks up its elements. Done on the loop,
//! a Take pressed while Preview was being redrawn waited behind it.
//!
//! So the slow half goes to a blocking worker (`Prebuilt::build`), and the
//! loop draws when it comes back:
//!
//! ```text
//!   loop: cells arrive -> which new chains? none -> draw now
//!                                           some -> worker builds them
//!   worker: Prebuilt::build -> Command::Preview(Built) -> loop
//!   loop: Built -> pool -> which new chains now? none -> draw with the pool
//!                                                some -> build those too
//! ```
//!
//! One build is out at a time. Cells that change while it is out are drawn
//! when it comes back, against what is armed then, so a slider dragged
//! during a build does not start a build per step and what was built is
//! never thrown away for being a step old. Chains the draw did not use are
//! dropped with the pool: a bin that never went into a pipeline needs
//! nothing more.

use super::Mixer;
use crate::multiview::preview_chain::Prebuilt;
use crate::preview::PreviewDemand;
use tracing::{debug, warn};

#[derive(Default)]
pub(super) struct Building {
    /// Built and not yet drawn.
    pool: Prebuilt,
    /// A worker is building now.
    out: bool,
}

impl Mixer {
    /// Draw the armed scene in the preview, now when nothing needs building
    /// and once the worker is done when something does.
    pub(super) fn draw_preview(&mut self) {
        // Drawn as the programme would draw it: a source that is down shows
        // its backup here too.
        let cells = self.preview_cells_now();
        let Some(mv) = self.multiview.as_mut() else { return };
        let pool = &self.preview_build.pool;
        let needs: Vec<_> = mv.preview_needs(&cells).into_iter().filter(|(k, w)| !pool.has(k, w)).collect();
        if !needs.is_empty() {
            mv.preview_filters_pending();
            if !self.preview_build.out {
                self.preview_build.out = true;
                debug!(chains = needs.len(), "building the preview's filters off the loop");
                let (canvas, handle) = (self.canvas.clone(), self.handle.clone());
                self.rt.spawn_blocking(move || {
                    let built = Prebuilt::build(needs, &canvas);
                    let _ = handle.send(super::Command::Preview(PreviewDemand::Built { built }));
                });
            }
            return;
        }
        let built = std::mem::take(&mut self.preview_build.pool);
        if let Err(e) = mv.apply_preview_built(&self.canvas, &cells, built) {
            warn!(?e, "the armed scene could not be drawn in the preview");
        }
        self.show_held_clips_again();
    }

    /// The worker is back. What it built goes in the pool and the preview is
    /// drawn against what is armed now.
    pub(super) fn preview_built(&mut self, built: Prebuilt) {
        self.preview_build.out = false;
        self.preview_build.pool.extend(built);
        self.draw_preview();
    }
}
