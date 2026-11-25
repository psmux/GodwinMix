//! A constant velocity Kalman filter for one box. Each of the centre, width
//! and height is its own position and velocity pair: the axes do not
//! correlate under constant velocity, so four 2 by 2 filters are exactly the
//! 8 state filter ByteTrack uses, at a fraction of the arithmetic.

use crate::geom::{Point, Rect};

/// Position and velocity on one axis, with their covariance.
#[derive(Clone, Copy, Debug)]
struct Axis {
    p: f32,
    v: f32,
    pp: f32,
    pv: f32,
    vv: f32,
}

impl Axis {
    fn new(p: f32, pos_var: f32, vel_var: f32) -> Self {
        Self { p, v: 0.0, pp: pos_var, pv: 0.0, vv: vel_var }
    }

    fn predict(&mut self, dt: f32, q_pos: f32, q_vel: f32) {
        self.p += self.v * dt;
        self.pp += 2.0 * dt * self.pv + dt * dt * self.vv + q_pos;
        self.pv += dt * self.vv;
        self.vv += q_vel;
    }

    fn update(&mut self, z: f32, r: f32) {
        let s = self.pp + r;
        let (kp, kv) = (self.pp / s, self.pv / s);
        let y = z - self.p;
        self.p += kp * y;
        self.v += kv * y;
        let (pp, pv) = (self.pp, self.pv);
        self.pp = (1.0 - kp) * pp;
        self.pv = (1.0 - kp) * pv;
        self.vv -= kv * pv;
    }
}

/// How much the filter trusts motion against measurement. The noise is a
/// fraction of the box height, as in ByteTrack, so a far away person and a
/// close one behave alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KalmanConfig {
    /// Position noise per second, as a fraction of the box height.
    pub position_noise: f32,
    /// Velocity noise per second, as a fraction of the box height.
    pub velocity_noise: f32,
    /// Measurement noise, as a fraction of the box height.
    pub measurement_noise: f32,
}

impl Default for KalmanConfig {
    fn default() -> Self {
        Self { position_noise: 0.25, velocity_noise: 0.5, measurement_noise: 0.05 }
    }
}

/// The filter for one tracked box: centre x, centre y, width and height.
#[derive(Clone, Copy, Debug)]
pub struct BoxFilter {
    axes: [Axis; 4],
    cfg: KalmanConfig,
}

impl BoxFilter {
    pub fn new(r: Rect, cfg: KalmanConfig) -> Self {
        let h = r.h.max(1e-3);
        let pos = (2.0 * cfg.measurement_noise * h).powi(2);
        let vel = (cfg.velocity_noise * h * 4.0).powi(2);
        let axes = [r.cx(), r.cy(), r.w, r.h].map(|p| Axis::new(p, pos, vel));
        Self { axes, cfg }
    }

    /// Moves the box forward by `dt` seconds.
    pub fn predict(&mut self, dt: f32) {
        let h = self.axes[3].p.max(1e-3);
        let q_pos = (self.cfg.position_noise * h).powi(2) * dt;
        let q_vel = (self.cfg.velocity_noise * h).powi(2) * dt;
        for a in &mut self.axes {
            a.predict(dt, q_pos, q_vel);
        }
        // A box never shrinks to nothing or turns inside out while coasting.
        for a in &mut self.axes[2..] {
            a.p = a.p.max(1e-3);
        }
    }

    /// Folds in a measured box.
    pub fn update(&mut self, r: Rect) {
        let rv = (self.cfg.measurement_noise * r.h.max(1e-3)).powi(2);
        for (a, z) in self.axes.iter_mut().zip([r.cx(), r.cy(), r.w, r.h]) {
            a.update(z, rv);
        }
    }

    /// Scales every velocity, for a track that has not been seen for a while
    /// and should drift to a stop rather than fly off.
    pub fn damp(&mut self, factor: f32) {
        for a in &mut self.axes {
            a.v *= factor;
        }
    }

    /// Carries the box with the picture when the camera moves: shifted by
    /// `dx`, `dy` and scaled by `scale` about the frame's centre. The
    /// velocity stays the person's own.
    pub fn carry(&mut self, dx: f32, dy: f32, scale: f32) {
        self.axes[0].p = 0.5 + (self.axes[0].p - 0.5) * scale + dx;
        self.axes[1].p = 0.5 + (self.axes[1].p - 0.5) * scale + dy;
        self.axes.iter_mut().for_each(|a| a.v *= scale);
        self.axes[2].p *= scale;
        self.axes[3].p *= scale;
    }

    pub fn rect(&self) -> Rect {
        let [cx, cy, w, h] = self.axes.map(|a| a.p);
        Rect::from_center(cx, cy, w, h)
    }

    /// The velocity of the centre, in normalised units per second.
    pub fn velocity(&self) -> Point {
        Point::new(self.axes[0].v, self.axes[1].v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_a_constant_velocity_and_predicts_with_it() {
        let mut f = BoxFilter::new(Rect::new(0.1, 0.4, 0.1, 0.3), KalmanConfig::default());
        let dt = 1.0 / 30.0;
        for i in 1..60 {
            f.predict(dt);
            f.update(Rect::new(0.1 + 0.2 * dt * i as f32, 0.4, 0.1, 0.3));
        }
        assert!((f.velocity().x - 0.2).abs() < 0.02, "{:?}", f.velocity());
        f.predict(0.5);
        let expect = 0.1 + 0.2 * (59.0 * dt + 0.5) + 0.05;
        assert!((f.rect().cx() - expect).abs() < 0.02, "{} vs {expect}", f.rect().cx());
    }
}
