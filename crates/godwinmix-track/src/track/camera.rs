//! The tracker on a moving camera. Each prediction carries every box with
//! the picture by the camera's [`EgoMotion`] over the step, so a pan is not
//! read as everyone walking the other way: ids hold, and a track's velocity
//! stays the person's own.

use std::time::Instant;

use super::{Track, Tracker};
use crate::detect::Detection;
use crate::ego::EgoMotion;
use crate::frame::FrameRef;

impl Tracker {
    /// As [`update_with_frame`](Tracker::update_with_frame), on a camera
    /// that moved as `ego` says since the last call.
    pub fn update_with_camera(&mut self, dets: &[Detection], frame: Option<&FrameRef>, t: Instant, ego: EgoMotion) -> &[Track] {
        self.ego = ego;
        self.update_with_frame(dets, frame, t);
        self.ego = EgoMotion::STILL;
        &self.out
    }

    /// As [`coast`](Tracker::coast), on a camera that moved as `ego` says.
    pub fn coast_with_camera(&mut self, t: Instant, ego: EgoMotion) -> &[Track] {
        self.ego = ego;
        self.coast(t);
        self.ego = EgoMotion::STILL;
        &self.out
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::geom::Rect;

    /// Five people standing still, seen by a camera panning right at half a
    /// frame a second: they slide left across the picture.
    fn pan(ego: EgoMotion) -> (Vec<Track>, u32) {
        let (mut tr, t0) = (Tracker::default(), Instant::now());
        let mut dets = Vec::new();
        for n in 0..50 {
            let shift = -0.5 * n as f32 / 25.0;
            dets.clear();
            dets.extend((0..5).map(|i| Detection::new(Rect::new(0.12 + 0.17 * i as f32 + shift + 0.3, 0.3, 0.06, 0.4), 0.9)));
            let ego = if n == 0 { EgoMotion::STILL } else { ego };
            tr.update_with_camera(&dets, None, t0 + Duration::from_millis(40 * n), ego);
        }
        let most = tr.tracks().iter().map(|t| t.id.0).max().unwrap_or(0);
        (tr.tracks().to_vec(), most)
    }

    #[test]
    fn people_standing_still_have_no_velocity_while_the_camera_pans() {
        let (tracks, _) = pan(EgoMotion { dx: -0.5, dy: 0.0, zoom: 0.0 });
        assert!(!tracks.is_empty());
        for t in &tracks {
            assert!(t.velocity.x.abs() < 0.03, "{} moves at {:?} though everyone stands still", t.id, t.velocity);
        }
        let (blind, _) = pan(EgoMotion::STILL);
        assert!(blind.iter().all(|t| t.velocity.x < -0.3), "without the camera's motion the pan reads as walking left");
    }
}
