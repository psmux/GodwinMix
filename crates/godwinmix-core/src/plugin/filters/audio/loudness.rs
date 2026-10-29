//! `loudness/filter`: ride the level towards a loudness target, in LUFS.
//!
//! The loudness is ITU-R BS.1770's: the sound K-weighted (a +4 dB shelf above
//! 1.7 kHz and a high pass at 38 Hz, as two biquads), its mean square taken
//! over 100 ms blocks and over the last three seconds of them, EBU R128's
//! short-term loudness. The gain moves towards `target_lufs` by at most
//! `speed_db_per_s`, within `max_boost_db` and `max_cut_db`, and holds while
//! the sound is under `gate_lufs` so a pause never pumps the room up. YouTube
//! and Spotify play at about -14, a podcast at -16, EBU broadcast at -23.
//! Put a limiter after it: this follows the average and lets a peak through.

use super::biquad::Biquad;
use super::run::Dsp;
use super::values::{db_to_gain, num};
use crate::config::Params;
use anyhow::Result;
use std::collections::VecDeque;

const KIND: &str = "loudness/filter";
/// EBU R128's short-term window, in 100 ms blocks.
const WINDOW: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Settings {
    target: f64,
    max_boost: f64,
    max_cut: f64,
    speed: f64,
    gate: f64,
}

pub struct Loudness {
    s: Settings,
    weight: Vec<Biquad>,
    scratch: Vec<f32>,
    rate: u32,
    block_len: usize,
    in_block: usize,
    block_sum: f64,
    blocks: VecDeque<f64>,
    gain_db: f64,
}

impl Default for Loudness {
    fn default() -> Self {
        let s = Settings { target: -14.0, max_boost: 12.0, max_cut: 12.0, speed: 2.0, gate: -50.0 };
        Loudness { s, weight: Vec::new(), scratch: Vec::new(), rate: 48000, block_len: 4800, in_block: 0, block_sum: 0.0, blocks: VecDeque::new(), gain_db: 0.0 }
    }
}

impl Loudness {
    /// Short-term loudness of what has been heard, in LUFS.
    pub fn short_term(&self) -> Option<f64> {
        if self.blocks.is_empty() {
            return None;
        }
        let mean = self.blocks.iter().sum::<f64>() / self.blocks.len() as f64;
        Some(-0.691 + 10.0 * mean.max(1e-12).log10())
    }

    /// K-weighted mean square of one frame, summed over channels.
    fn measure(&mut self, samples: &[f32], channels: usize) {
        self.scratch.clear();
        self.scratch.extend_from_slice(samples);
        for stage in &mut self.weight {
            stage.process(&mut self.scratch, channels);
        }
        for frame in self.scratch.chunks_exact(channels) {
            self.block_sum += frame.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>();
            self.in_block += 1;
            if self.in_block == self.block_len {
                self.blocks.push_back(self.block_sum / self.block_len as f64);
                if self.blocks.len() > WINDOW {
                    self.blocks.pop_front();
                }
                self.in_block = 0;
                self.block_sum = 0.0;
            }
        }
    }
}

/// BS.1770's two stages designed for `rate`, which at 48 kHz are exactly the
/// coefficients the recommendation prints. The shelf is not the cookbook one:
/// its zero gain is `Vh^0.4997` where the cookbook has the square root, and
/// the cookbook's reads a 1 kHz tone a quarter of a decibel low.
fn k_weighting(rate: u32, channels: usize) -> Vec<Biquad> {
    let fs = rate.max(1) as f64;
    let k = (std::f64::consts::PI * 1681.974_450_955_532 / fs).tan();
    let (q, vh) = (0.707_175_236_955_419_3, 10f64.powf(3.999_843_853_97 / 20.0));
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad::from_coeffs(
        [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
        [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        channels,
    );
    let k = (std::f64::consts::PI * 38.135_470_876_139_82 / fs).tan();
    let q = 0.500_327_037_325_395_3;
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad::from_coeffs([1.0, -2.0, 1.0], [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0], channels);
    vec![shelf, high_pass]
}

impl Dsp for Loudness {
    fn set(&mut self, params: &Params) -> Result<()> {
        self.s = Settings {
            target: num(KIND, params, &["target_lufs", "target"], -14.0, -40.0, -5.0)?,
            max_boost: num(KIND, params, &["max_boost_db"], 12.0, 0.0, 30.0)?,
            max_cut: num(KIND, params, &["max_cut_db"], 12.0, 0.0, 30.0)?,
            speed: num(KIND, params, &["speed_db_per_s"], 2.0, 0.1, 20.0)?,
            gate: num(KIND, params, &["gate_lufs"], -50.0, -70.0, -20.0)?,
        };
        Ok(())
    }

    fn prepare(&mut self, rate: u32, channels: usize) {
        self.rate = rate;
        self.block_len = (rate as usize / 10).max(1);
        self.weight = k_weighting(rate, channels);
        self.blocks.clear();
        self.in_block = 0;
        self.block_sum = 0.0;
    }

    fn process(&mut self, samples: &mut [f32], channels: usize) {
        self.measure(samples, channels);
        let from = self.gain_db;
        if let Some(lufs) = self.short_term().filter(|l| *l > self.s.gate) {
            let want = (self.s.target - lufs).clamp(-self.s.max_cut, self.s.max_boost);
            let frames = (samples.len() / channels.max(1)) as f64;
            let step = self.s.speed * frames / self.rate.max(1) as f64;
            self.gain_db += (want - self.gain_db).clamp(-step, step);
        }
        super::gain::ramp(samples, channels, db_to_gain(from), db_to_gain(self.gain_db));
    }
}

#[cfg(test)]
#[path = "loudness_tests.rs"]
mod tests;
