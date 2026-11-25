//! A smoothed crop rectangle for a digital camera: the centre and the height
//! each follow the target on a critically damped spring, so the virtual
//! camera eases in and out and never overshoots.

use std::time::Instant;

use crate::geom::Rect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropConfig {
    /// Roughly how long, in seconds, the crop takes to reach a new target.
    pub smooth_time: f32,
    /// The fastest the centre may move, normalised units per second.
    pub max_speed: f32,
    /// A target that moves less than this from the crop's goal is ignored,
    /// so a person shifting on their feet does not sway the shot.
    pub dead_zone: f32,
}

impl Default for CropConfig {
    fn default() -> Self {
        Self { smooth_time: 0.6, max_speed: 1.0, dead_zone: 0.03 }
    }
}

/// One critically damped value (the smooth damp of game cameras).
#[derive(Clone, Copy, Debug, Default)]
struct Spring {
    pos: f32,
    vel: f32,
}

impl Spring {
    fn step(&mut self, target: f32, smooth: f32, max_speed: f32, dt: f32) {
        let omega = 2.0 / smooth.max(1e-3);
        let x = omega * dt;
        let decay = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
        let max_change = max_speed * smooth;
        let before = self.pos;
        let change = (before - target).clamp(-max_change, max_change);
        let goal = before - change;
        let temp = (self.vel + omega * change) * dt;
        self.vel = (self.vel - omega * temp) * decay;
        self.pos = goal + (change + temp) * decay;
        // Never pass the target: arriving is better than ringing round it.
        if (target > before) == (self.pos > target) && dt > 0.0 {
            self.pos = target;
            self.vel = 0.0;
        }
    }
}

/// Eases a crop rectangle towards each frame's target.
#[derive(Clone, Debug)]
pub struct CropSteer {
    pub config: CropConfig,
    cx: Spring,
    cy: Spring,
    h: Spring,
    goal: Rect,
    aspect: f32,
    last_t: Option<Instant>,
}

impl CropSteer {
    /// Starts on the whole frame.
    pub fn new(config: CropConfig) -> Self {
        let mut s = Self {
            config,
            cx: Spring::default(),
            cy: Spring::default(),
            h: Spring::default(),
            goal: Rect::FULL,
            aspect: 1.0,
            last_t: None,
        };
        s.jump(Rect::FULL);
        s
    }

    /// Goes straight to `r`, for a cut.
    pub fn jump(&mut self, r: Rect) {
        let r = r.clamp_inside();
        (self.cx, self.cy, self.h) = (Spring { pos: r.cx(), vel: 0.0 }, Spring { pos: r.cy(), vel: 0.0 }, Spring { pos: r.h, vel: 0.0 });
        self.aspect = r.w / r.h.max(1e-6);
        self.goal = r;
    }

    /// The crop for this frame. `None` holds the current goal.
    pub fn step(&mut self, target: Option<Rect>, t: Instant) -> Rect {
        let dt = self.last_t.map_or(0.0, |l| t.saturating_duration_since(l).as_secs_f32()).min(0.5);
        self.last_t = Some(t);
        if let Some(r) = target.filter(Rect::is_finite) {
            let r = r.clamp_inside();
            let moved = r.center().distance(self.goal.center());
            let resized = (r.h - self.goal.h).abs() / self.goal.h.max(1e-6);
            if moved > self.config.dead_zone || resized > self.config.dead_zone * 3.0 {
                self.goal = r;
                self.aspect = r.w / r.h.max(1e-6);
            }
        }
        let c = self.config;
        self.cx.step(self.goal.cx(), c.smooth_time, c.max_speed, dt);
        self.cy.step(self.goal.cy(), c.smooth_time, c.max_speed, dt);
        self.h.step(self.goal.h, c.smooth_time, c.max_speed, dt);
        self.current()
    }

    /// The crop as it stands, inside the frame.
    pub fn current(&self) -> Rect {
        let h = self.h.pos.max(1e-3);
        Rect::from_center(self.cx.pos, self.cy.pos, h * self.aspect, h).clamp_inside()
    }
}
