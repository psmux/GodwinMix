//! Fake cameras that keep a position and answer queries, the way a real one
//! does: a head that moves at the speed it was last told, presets that
//! remember where it was, focus and exposure that remember what was set.
//!
//! [`Head`] is the camera; [`visca`], [`onvif`] and [`canon`] put it behind
//! each protocol on a loopback socket, so a driver is tested over the same
//! bytes it sends to real hardware. [`FakePtz`] drives a head directly, which
//! is what the plugin's `fake` protocol uses for a demo with no camera.

pub mod canon;
mod direct;
pub mod http;
pub mod onvif;
pub mod visca;

pub use direct::FakePtz;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::types::{Exposure, ExposureMode, Focus, Position, Velocity};

/// How far the head goes in a second at full speed, in normalised units: pan
/// and tilt cross their whole range in two seconds, zoom in two.
pub const PAN_RATE: f32 = 1.0;
pub const ZOOM_RATE: f32 = 0.5;

#[derive(Debug, Clone)]
pub struct Head {
    pub pan: f32,
    pub tilt: f32,
    pub zoom: f32,
    pub velocity: Velocity,
    pub presets: BTreeMap<u16, (f32, f32, f32)>,
    pub focus_auto: bool,
    pub focus_drive: f32,
    pub exposure: Exposure,
    /// Every movement command this head was given, for a test to count.
    pub moves: u64,
    /// How long `save_preset` and `position` take, to stand in for a slow
    /// camera. Only [`FakePtz`] waits them out.
    pub save_delay: Duration,
    pub query_delay: Duration,
    at: Instant,
}

impl Default for Head {
    fn default() -> Head {
        Head {
            pan: 0.0,
            tilt: 0.0,
            zoom: 0.0,
            velocity: Velocity::STOP,
            presets: BTreeMap::new(),
            focus_auto: true,
            focus_drive: 0.0,
            exposure: Exposure { mode: Some(ExposureMode::Auto), ..Exposure::default() },
            moves: 0,
            save_delay: Duration::ZERO,
            query_delay: Duration::ZERO,
            at: Instant::now(),
        }
    }
}

pub type Shared = Arc<Mutex<Head>>;

pub fn shared() -> Shared {
    Arc::new(Mutex::new(Head::default()))
}

impl Head {
    /// Move the head on by the time since it was last looked at.
    pub fn advance(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.at).as_secs_f32();
        self.at = now;
        self.pan = (self.pan + self.velocity.pan * PAN_RATE * dt).clamp(-1.0, 1.0);
        self.tilt = (self.tilt + self.velocity.tilt * PAN_RATE * dt).clamp(-1.0, 1.0);
        self.zoom = (self.zoom + self.velocity.zoom * ZOOM_RATE * dt).clamp(0.0, 1.0);
    }

    pub fn set_velocity(&mut self, v: Velocity) {
        self.advance();
        self.velocity = v;
        self.moves += 1;
    }

    pub fn jump(&mut self, pan: f32, tilt: f32, zoom: f32) {
        self.advance();
        self.velocity = Velocity::STOP;
        (self.pan, self.tilt, self.zoom) = (pan.clamp(-1.0, 1.0), tilt.clamp(-1.0, 1.0), zoom.clamp(0.0, 1.0));
    }

    pub fn save(&mut self, n: u16) {
        self.advance();
        self.presets.insert(n, (self.pan, self.tilt, self.zoom));
    }

    /// Recall preset `n`. A preset nobody saved leaves the head where it is,
    /// as a real camera does.
    pub fn recall(&mut self, n: u16) -> bool {
        match self.presets.get(&n).copied() {
            Some((p, t, z)) => {
                self.jump(p, t, z);
                true
            }
            None => false,
        }
    }

    pub fn position(&mut self) -> Position {
        self.advance();
        Position { pan: self.pan, tilt: self.tilt, zoom: self.zoom, raw: [0; 3] }
    }

    pub fn focus(&mut self, f: Focus) {
        match f {
            Focus::Auto => self.focus_auto = true,
            Focus::Manual | Focus::OnePush => self.focus_auto = false,
            Focus::Drive(s) => self.focus_drive = s,
        }
    }
}
