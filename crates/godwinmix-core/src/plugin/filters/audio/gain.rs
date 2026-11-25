//! `gain/filter`: a fixed boost or cut, in decibels.
//!
//! A change of setting is ramped across one buffer, so a fader moved on air
//! never clicks.

use super::run::Dsp;
use super::values::{db_to_gain, num};
use crate::config::Params;
use anyhow::Result;

const KIND: &str = "gain/filter";

pub struct Gain {
    target: f32,
    now: f32,
}

impl Default for Gain {
    fn default() -> Self {
        Gain { target: 1.0, now: 1.0 }
    }
}

impl Dsp for Gain {
    fn set(&mut self, params: &Params) -> Result<()> {
        self.target = db_to_gain(num(KIND, params, &["gain_db", "db"], 0.0, -60.0, 30.0)?);
        Ok(())
    }

    fn prepare(&mut self, _rate: u32, _channels: usize) {
        self.now = self.target;
    }

    fn process(&mut self, samples: &mut [f32], channels: usize) {
        ramp(samples, channels, self.now, self.target);
        self.now = self.target;
    }
}

/// Multiply by a gain that moves from `from` to `to` across the buffer.
pub(crate) fn ramp(samples: &mut [f32], channels: usize, from: f32, to: f32) {
    if from == to {
        if to != 1.0 {
            samples.iter_mut().for_each(|s| *s *= to);
        }
        return;
    }
    let frames = (samples.len() / channels.max(1)).max(1);
    let step = (to - from) / frames as f32;
    for (n, frame) in samples.chunks_exact_mut(channels.max(1)).enumerate() {
        let g = from + step * (n + 1) as f32;
        frame.iter_mut().for_each(|s| *s *= g);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_decibels_down_halves_the_level() {
        let mut g = Gain::default();
        g.set(&toml::from_str("gain_db = -6.0206").unwrap()).unwrap();
        g.prepare(48000, 2);
        let mut s = vec![0.8f32; 8];
        g.process(&mut s, 2);
        assert!(s.iter().all(|v| (v - 0.4).abs() < 1e-3), "{s:?}");
    }

    #[test]
    fn a_change_on_air_is_ramped_not_stepped() {
        let mut g = Gain::default();
        g.prepare(48000, 1);
        g.set(&toml::from_str("db = -60").unwrap()).unwrap();
        let mut s = vec![1.0f32; 100];
        g.process(&mut s, 1);
        assert!(s[0] > 0.9 && s[99] < 0.01, "first {} last {}", s[0], s[99]);
        assert!(s.windows(2).all(|w| w[1] <= w[0]));
    }
}
