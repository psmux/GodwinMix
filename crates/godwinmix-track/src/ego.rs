//! The camera's own motion, as the picture sees it. A PTZ camera that pans
//! moves everything in its picture the other way; without knowing that, a
//! tracker reads the pan as every person walking, and a motion detector sees
//! the whole frame change. The follow loop knows what speed it last sent
//! the camera, and [`CameraOptics`] turns that into an [`EgoMotion`].

use crate::steer::PtzVelocity;

/// How the picture moves because the camera moves: everything in it shifts
/// by `dx`, `dy` a second (normalised: 1 is the frame's width or height),
/// and grows about the centre by `e^zoom` a second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EgoMotion {
    pub dx: f32,
    pub dy: f32,
    pub zoom: f32,
}

impl EgoMotion {
    pub const STILL: EgoMotion = EgoMotion { dx: 0.0, dy: 0.0, zoom: 0.0 };

    pub fn is_still(&self) -> bool {
        *self == Self::STILL
    }

    /// How fast the picture's content moves, in frames a second: the larger
    /// shift, plus how far a zoom moves the frame's edge.
    pub fn speed(&self) -> f32 {
        self.dx.abs().max(self.dy.abs()) + self.zoom.abs() * 0.5
    }

    /// Over `dt` seconds: the shift and the scale.
    pub fn over(&self, dt: f32) -> (f32, f32, f32) {
        (self.dx * dt, self.dy * dt, (self.zoom * dt).exp())
    }
}

/// What turns a camera speed into picture motion. Each camera differs. The
/// field of view is the one when a follow starts (the zoom the camera
/// mostly tracks at); the follower follows its own zoom commands from there.
/// A pan the optics get a quarter wrong is still three quarters taken out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraOptics {
    /// Horizontal and vertical field of view, degrees.
    pub hfov_deg: f32,
    pub vfov_deg: f32,
    /// Degrees a second at full pan speed (1.0), and at full tilt speed.
    pub pan_deg_s: f32,
    pub tilt_deg_s: f32,
    /// The natural log of how much the picture grows in a second at full
    /// zoom speed: 0.7 doubles it in a second.
    pub zoom_rate: f32,
}

impl Default for CameraOptics {
    /// A 20x PTZ camera at a mid zoom: 30 by 17 degrees seen, 60 and 30
    /// degrees a second at full pan and tilt speed.
    fn default() -> Self {
        Self { hfov_deg: 30.0, vfov_deg: 17.0, pan_deg_s: 60.0, tilt_deg_s: 30.0, zoom_rate: 0.7 }
    }
}

impl CameraOptics {
    /// The picture motion for a camera moving at `v`. Panning right moves
    /// the picture left; tilting up moves it down; zooming in grows it.
    pub fn ego(&self, v: PtzVelocity) -> EgoMotion {
        EgoMotion {
            dx: -v.pan * self.pan_deg_s / self.hfov_deg.max(0.1),
            dy: v.tilt * self.tilt_deg_s / self.vfov_deg.max(0.1),
            zoom: v.zoom * self.zoom_rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pan_right_moves_the_picture_left_by_the_speed_over_the_field_of_view() {
        let o = CameraOptics { hfov_deg: 60.0, vfov_deg: 34.0, pan_deg_s: 30.0, tilt_deg_s: 17.0, zoom_rate: 0.7 };
        let e = o.ego(PtzVelocity { pan: 1.0, tilt: 1.0, zoom: 0.0 });
        assert_eq!((e.dx, e.dy), (-0.5, 0.5));
        assert_eq!(e.speed(), 0.5);
        assert!(o.ego(PtzVelocity::STOP).is_still());
        let (_, _, scale) = o.ego(PtzVelocity { pan: 0.0, tilt: 0.0, zoom: 1.0 }).over(1.0);
        assert!((scale - 2.0).abs() < 0.02);
    }
}
