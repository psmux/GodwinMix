//! Pan, tilt and zoom velocities from a target rectangle: a PD controller per
//! axis with a dead zone, a speed limit, an acceleration limit for ease in
//! and ease out, and a hold after settling so the camera does not hunt.

use std::time::{Duration, Instant};

use crate::geom::Rect;

/// Camera speeds, each -1 to 1 of the camera's top speed. Positive pan turns
/// right, positive tilt looks up, positive zoom goes in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PtzVelocity {
    pub pan: f32,
    pub tilt: f32,
    pub zoom: f32,
}

impl PtzVelocity {
    pub const STOP: PtzVelocity = PtzVelocity { pan: 0.0, tilt: 0.0, zoom: 0.0 };

    pub fn is_stopped(&self) -> bool {
        *self == Self::STOP
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PtzConfig {
    /// Speed per unit of error. The error is the target centre's offset from
    /// the frame centre (half the frame is 0.5) and, for zoom, the log of how
    /// much smaller the target is than the frame.
    pub gain: f32,
    pub zoom_gain: f32,
    /// Speed per unit of error change per second, which damps overshoot.
    pub damping: f32,
    /// Below this error an axis that is still stays still.
    pub dead_zone: f32,
    pub zoom_dead_zone: f32,
    /// A moving axis stops once its error is under this fraction of the dead
    /// zone, so it settles in the middle rather than at the edge.
    pub settle: f32,
    pub max_speed: f32,
    /// The most the speed may change in one second.
    pub max_accel: f32,
    /// How long a settled axis waits before it may move again.
    pub hold: Duration,
}

impl Default for PtzConfig {
    fn default() -> Self {
        Self {
            gain: 2.0,
            zoom_gain: 0.8,
            damping: 0.15,
            dead_zone: 0.06,
            zoom_dead_zone: 0.15,
            settle: 0.4,
            max_speed: 0.8,
            max_accel: 2.5,
            hold: Duration::from_millis(600),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AxisCtl {
    moving: bool,
    speed: f32,
    last_err: Option<f32>,
    settled_at: Option<Instant>,
}

impl AxisCtl {
    fn step(&mut self, err: Option<f32>, gain: f32, dead: f32, cfg: &PtzConfig, t: Instant, dt: f32) -> f32 {
        let desired = match err {
            None => {
                self.moving = false;
                self.last_err = None;
                0.0
            }
            Some(e) => self.desired(e, gain, dead, cfg, t, dt),
        };
        let step = cfg.max_accel * dt;
        // Never jump past zero in one step: slowing down and turning round
        // are two moves, so the camera does not jerk.
        let next = self.speed + (desired - self.speed).clamp(-step, step);
        self.speed = if next * self.speed < 0.0 { 0.0 } else { next };
        self.speed
    }

    fn desired(&mut self, e: f32, gain: f32, dead: f32, cfg: &PtzConfig, t: Instant, dt: f32) -> f32 {
        let rate = match (self.last_err, dt > 0.0) {
            (Some(l), true) => (e - l) / dt,
            _ => 0.0,
        };
        self.last_err = Some(e);
        if self.moving && e.abs() < dead * cfg.settle {
            self.moving = false;
            self.settled_at = Some(t);
        } else if !self.moving && e.abs() > dead {
            let held = self.settled_at.is_none_or(|s| t.saturating_duration_since(s) >= cfg.hold);
            self.moving = held;
        }
        if !self.moving {
            return 0.0;
        }
        (gain * e + cfg.damping * rate).clamp(-cfg.max_speed, cfg.max_speed)
    }
}

/// Turns a target rectangle, in the camera's own picture, into speeds.
#[derive(Clone, Debug)]
pub struct PtzSteer {
    pub config: PtzConfig,
    axes: [AxisCtl; 3],
    last_t: Option<Instant>,
}

impl PtzSteer {
    pub fn new(config: PtzConfig) -> Self {
        Self { config, axes: [AxisCtl::default(); 3], last_t: None }
    }

    /// The speeds for this frame. `None` eases every axis to a stop.
    pub fn step(&mut self, target: Option<Rect>, t: Instant) -> PtzVelocity {
        let dt = self.last_t.map_or(0.0, |l| t.saturating_duration_since(l).as_secs_f32()).min(0.5);
        self.last_t = Some(t);
        let c = self.config;
        let target = target.filter(Rect::is_finite);
        let pan = target.map(|r| r.cx() - 0.5);
        let tilt = target.map(|r| 0.5 - r.cy());
        let zoom = target.map(|r| -r.h.max(1e-3).ln());
        PtzVelocity {
            pan: self.axes[0].step(pan, c.gain, c.dead_zone, &c, t, dt),
            tilt: self.axes[1].step(tilt, c.gain, c.dead_zone, &c, t, dt),
            zoom: self.axes[2].step(zoom, c.zoom_gain, c.zoom_dead_zone, &c, t, dt),
        }
    }

    /// Stops at once, for a cut or a manual move.
    pub fn reset(&mut self) {
        self.axes = [AxisCtl::default(); 3];
        self.last_t = None;
    }
}
