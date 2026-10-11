//! A source as it is drawn: itself, or what stands in for it.

use super::super::slots::Placement;
use super::super::{Mixer, FREEZE_HOLD};
use crate::state::SourceId;
use std::time::Duration;
use tracing::warn;

impl Mixer {
    /// A placement as it is drawn, with its source and any group child
    /// swapped for what stands in for it.
    pub(in crate::mixer) fn stood_in(&self, p: &Placement) -> Placement {
        let mut p = p.clone();
        if !self.backups.busy() {
            return p;
        }
        if let Some(by) = self.backups.stand_in(&p.source) {
            p.source = by.into();
        }
        for child in &mut p.group {
            if let Some(by) = self.backups.stand_in(&child.source) {
                child.source = by.into();
            }
        }
        p
    }

    /// A source id as it is drawn.
    pub(in crate::mixer) fn stand_in_for(&self, id: &SourceId) -> SourceId {
        self.backups.stand_in(id).map(SourceId::from).unwrap_or_else(|| id.clone())
    }

    /// How long a source's last frame stays up: the usual hold, or longer
    /// when its backup waits longer than that before going on, so the
    /// picture does not drop to the slate in between.
    pub(in crate::mixer) fn hold_for(&self, id: &str) -> Duration {
        let Some(rule) = self.backups.rule(id) else { return FREEZE_HOLD };
        let wait = rule.after_secs.clamp(0.0, godwinmix_protocol::backup::MAX_WAIT_SECS);
        FREEZE_HOLD.max(Duration::from_secs_f64(wait) + Duration::from_secs(1))
    }

    /// The armed scene, drawn in the preview as the programme would draw it.
    pub(in crate::mixer) fn preview_cells_now(&self) -> Vec<crate::multiview::preview::Cell> {
        let mut cells = self.preview_cells.clone();
        if self.backups.busy() {
            for cell in &mut cells {
                cell.source = self.stand_in_for(&cell.source);
            }
        }
        cells
    }

    pub(super) fn redraw_preview(&mut self) {
        if self.multiview.is_none() || self.preview_cells.is_empty() {
            return;
        }
        let cells = self.preview_cells_now();
        if let Some(mv) = self.multiview.as_mut() {
            if let Err(e) = mv.apply_preview(&self.canvas, &cells) {
                warn!(?e, "the armed scene could not be drawn again with its backups");
            }
        }
    }
}
