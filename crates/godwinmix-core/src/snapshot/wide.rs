//! A source's still wider than its cell.
//!
//! A source's still is cut from its cell on the mosaic, and the cell is as
//! wide as the layout makes it: 480 pixels for two cells on the default 960
//! wide mosaic, 320 for three columns. A wider `width` came back at the cell's
//! width. So a request wider than its cell holds the mosaic wider, up to the
//! widest mosaic there is (1920), for as long as the tracker follows, and the
//! still is cut from a frame drawn at that size. The programme does not come
//! this way: it has a branch of its own (`Tracker::programme`).

use super::{Latest, Pick, Tracker};
use crate::multiview::{MultiviewRequest, MultiviewSubscription};
use crate::state::CellAssignment;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The widest mosaic a still may ask for. The same as the mosaic's own limit.
pub const MOSAIC_MAX: i32 = 1920;

/// How long a still waits for the mosaic to be rebuilt wider.
const REBUILD_WAIT: Duration = Duration::from_secs(4);

/// The wider subscription, held until the tracker lets the mosaic go.
pub(super) type Held = parking_lot::Mutex<Option<(i32, MultiviewSubscription)>>;

/// The mosaic width that would make `cell` `width` across, now that the
/// mosaic is `mosaic` wide, up to [`MOSAIC_MAX`].
pub fn mosaic_for(width: u32, cell: &CellAssignment, mosaic: i32) -> i32 {
    let cell_w = cell.w.max(1) as i64;
    let needed = (i64::from(mosaic) * i64::from(width) + cell_w - 1) / cell_w;
    needed.clamp(i64::from(mosaic), i64::from(MOSAIC_MAX)) as i32
}

impl Tracker {
    /// The newest frame with `pick`'s cell at least `width` wide, or as wide as
    /// the widest mosaic allows. `latest` is what the caller already has.
    pub async fn wide_enough(self: &Arc<Self>, pick: &Pick, width: Option<u32>, latest: Latest) -> Latest {
        let (Some(width), Pick::Source(_)) = (width, pick) else { return latest };
        let Some(cell) = super::find_cell(&latest.cells, pick).cloned() else { return latest };
        let Some(shape) = self.mv.shape() else { return latest };
        if (cell.w.max(0) as u32) >= width || shape.width >= MOSAIC_MAX {
            return latest;
        }
        let target = mosaic_for(width, &cell, shape.width);
        self.hold_wide(target);
        let enough = (cell.w as i64 * i64::from(target) / i64::from(shape.width.max(1))) as i32 - 2;
        let deadline = Instant::now() + REBUILD_WAIT;
        // Only a frame drawn with every tile in it: the first frames of the
        // rebuilt mosaic are black where a tile has not delivered yet.
        while Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
            self.want();
            let drawn = self.settled.load(std::sync::atomic::Ordering::Acquire) && self.mv.warm();
            if let Some(now) = self.latest().filter(|_| drawn) {
                if super::find_cell(&now.cells, pick).is_some_and(|c| c.w >= enough) {
                    return now;
                }
            }
        }
        // The narrower still is still a true one.
        latest
    }

    /// Keep a subscription at `width` across until the tracker goes idle.
    fn hold_wide(&self, width: i32) {
        let mut held = self.wide.lock();
        if held.as_ref().is_some_and(|(w, _)| *w >= width) {
            return;
        }
        *held = Some((width, self.mv.subscribe(MultiviewRequest { fps: 0, width })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(w: i32) -> CellAssignment {
        CellAssignment { index: 1, source: Some("cam".into()), x: 0, y: 0, w, h: w * 9 / 16 }
    }

    #[test]
    fn the_mosaic_is_asked_for_just_wide_enough_and_never_past_its_limit() {
        assert_eq!(mosaic_for(960, &cell(480), 960), 1920);
        assert_eq!(mosaic_for(640, &cell(480), 960), 1280);
        assert_eq!(mosaic_for(1280, &cell(320), 960), 1920, "capped at the widest mosaic");
        assert_eq!(mosaic_for(100, &cell(320), 960), 960, "never narrower than it is");
    }
}
