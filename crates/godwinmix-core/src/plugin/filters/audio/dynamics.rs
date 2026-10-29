//! `compressor/filter`, `expander/filter` and `limiter/filter`: one gain
//! computer with three curves.
//!
//! The level is the loudest channel of each frame, so a stereo image never
//! shifts when one side is louder. The gain reduction is smoothed with the
//! attack time when it deepens and the release time when it lets go. The
//! limiter's attack is instant, so nothing leaves it above its ceiling.
//! OBS's own names (`threshold`, `ratio`, `attack_time`, `release_time`,
//! `output_gain`) are read as well.

use super::run::Dsp;
use super::values::{db_to_gain, num, time_coeff};
use crate::config::Params;
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Compress,
    Expand,
    Limit,
}

impl Mode {
    fn kind(self) -> &'static str {
        match self {
            Mode::Compress => "compressor/filter",
            Mode::Expand => "expander/filter",
            Mode::Limit => "limiter/filter",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Settings {
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    attack_ms: f64,
    release_ms: f64,
    output_db: f64,
}

pub struct Dynamics {
    mode: Mode,
    s: Settings,
    rate: u32,
    attack: f32,
    release: f32,
    /// The gain change now, in decibels, zero or below.
    gr_db: f32,
}

impl Dynamics {
    pub fn new(mode: Mode) -> Dynamics {
        let s = defaults(mode);
        Dynamics { mode, s, rate: 48000, attack: 0.0, release: 0.0, gr_db: 0.0 }
    }

    /// The gain change the curve asks for at this level.
    fn curve(&self, level_db: f32) -> f32 {
        let s = &self.s;
        let over = level_db - s.threshold_db;
        match self.mode {
            Mode::Limit => over.max(0.0) * -1.0,
            Mode::Expand => (over.min(0.0) * (s.ratio - 1.0)).max(-80.0),
            Mode::Compress => {
                let slope = 1.0 / s.ratio - 1.0;
                let knee = s.knee_db;
                if knee > 0.0 && (2.0 * over).abs() <= knee {
                    slope * (over + knee / 2.0).powi(2) / (2.0 * knee)
                } else if over > 0.0 {
                    slope * over
                } else {
                    0.0
                }
            }
        }
    }
}

fn defaults(mode: Mode) -> Settings {
    match mode {
        Mode::Compress => Settings { threshold_db: -18.0, ratio: 4.0, knee_db: 6.0, attack_ms: 6.0, release_ms: 60.0, output_db: 0.0 },
        Mode::Expand => Settings { threshold_db: -40.0, ratio: 2.0, knee_db: 0.0, attack_ms: 10.0, release_ms: 50.0, output_db: 0.0 },
        Mode::Limit => Settings { threshold_db: -1.0, ratio: f32::INFINITY, knee_db: 0.0, attack_ms: 0.0, release_ms: 60.0, output_db: 0.0 },
    }
}

impl Dsp for Dynamics {
    fn set(&mut self, params: &Params) -> Result<()> {
        let k = self.mode.kind();
        let d = defaults(self.mode);
        let max_ratio = if self.mode == Mode::Expand { 20.0 } else { 32.0 };
        let s = Settings {
            threshold_db: num(k, params, &["threshold_db", "threshold"], d.threshold_db as f64, -60.0, 0.0)? as f32,
            ratio: if self.mode == Mode::Limit {
                d.ratio
            } else {
                num(k, params, &["ratio"], d.ratio as f64, 1.0, max_ratio)? as f32
            },
            knee_db: num(k, params, &["knee_db"], d.knee_db as f64, 0.0, 24.0)? as f32,
            attack_ms: if self.mode == Mode::Limit {
                0.0
            } else {
                num(k, params, &["attack_ms", "attack_time"], d.attack_ms, 0.0, 500.0)?
            },
            release_ms: num(k, params, &["release_ms", "release_time"], d.release_ms, 1.0, 2000.0)?,
            output_db: num(k, params, &["output_gain_db", "output_gain"], d.output_db, -32.0, 32.0)?,
        };
        self.s = s;
        self.prepare(self.rate, 0);
        Ok(())
    }

    fn prepare(&mut self, rate: u32, _channels: usize) {
        self.rate = rate;
        self.attack = time_coeff(self.s.attack_ms, rate);
        self.release = time_coeff(self.s.release_ms, rate);
    }

    fn process(&mut self, samples: &mut [f32], channels: usize) {
        let out = db_to_gain(self.s.output_db);
        let ceiling = db_to_gain(self.s.threshold_db as f64);
        for frame in samples.chunks_exact_mut(channels) {
            let peak = frame.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let want = self.curve(20.0 * peak.max(1e-6).log10());
            let c = if want < self.gr_db { self.attack } else { self.release };
            self.gr_db = want + c * (self.gr_db - want);
            let g = 10f32.powf(self.gr_db / 20.0) * out;
            for x in frame.iter_mut() {
                *x *= g;
                if self.mode == Mode::Limit {
                    *x = x.clamp(-ceiling, ceiling);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "dynamics_tests.rs"]
mod tests;
