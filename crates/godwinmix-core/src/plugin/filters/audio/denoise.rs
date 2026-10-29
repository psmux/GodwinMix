//! `denoise/filter`: take fans, hum and traffic out from under a voice.
//!
//! RNNoise, as the pure Rust `nnnoiseless`: a small recurrent network run on
//! the CPU, the same model OBS ships as its RNNoise choice. It works on 10 ms
//! frames at 48 kHz, one state per channel, so it adds 10 ms and needs the
//! canvas at 48000 Hz. `amount` from 0 to 1 mixes the cleaned sound with the
//! original; 1, the default, is all cleaned.
//!
//! Behind the `denoise` feature, which the binaries turn on. Each channel
//! costs a few percent of one core on a Raspberry Pi 4 and nothing at all
//! until the filter is added.

use super::run::Dsp;
use super::values::num;
use crate::config::Params;
use anyhow::{bail, Result};
use nnnoiseless::DenoiseState;
use std::collections::VecDeque;

const KIND: &str = "denoise/filter";
const FRAME: usize = DenoiseState::FRAME_SIZE;
const RATE: u32 = 48000;

struct Channel {
    state: Box<DenoiseState<'static>>,
    input: Vec<f32>,
    cleaned: Vec<f32>,
    output: VecDeque<f32>,
}

impl Channel {
    fn new() -> Channel {
        Channel {
            state: DenoiseState::new(),
            input: Vec::with_capacity(FRAME),
            cleaned: vec![0.0; FRAME],
            // One frame of silence ahead, so a sample always has one to
            // come out in its place: that frame is the 10 ms this costs.
            output: std::iter::repeat_n(0.0, FRAME).collect(),
        }
    }

    fn push(&mut self, x: f32, amount: f32) -> f32 {
        self.input.push(x * 32768.0);
        if self.input.len() == FRAME {
            self.state.process_frame(&mut self.cleaned, &self.input);
            for (c, o) in self.cleaned.iter().zip(&self.input) {
                self.output.push_back((c * amount + o * (1.0 - amount)) / 32768.0);
            }
            self.input.clear();
        }
        self.output.pop_front().unwrap_or(0.0)
    }
}

pub struct Denoise {
    amount: f32,
    channels: Vec<Channel>,
}

impl Default for Denoise {
    fn default() -> Self {
        Denoise { amount: 1.0, channels: Vec::new() }
    }
}

impl Dsp for Denoise {
    fn set(&mut self, params: &Params) -> Result<()> {
        self.amount = num(KIND, params, &["amount"], 1.0, 0.0, 1.0)? as f32;
        Ok(())
    }

    fn prepare(&mut self, _rate: u32, channels: usize) {
        self.channels = (0..channels).map(|_| Channel::new()).collect();
    }

    fn process(&mut self, samples: &mut [f32], channels: usize) {
        if self.channels.len() != channels {
            self.prepare(RATE, channels);
        }
        let amount = self.amount;
        for frame in samples.chunks_exact_mut(channels) {
            for (x, ch) in frame.iter_mut().zip(self.channels.iter_mut()) {
                *x = ch.push(*x, amount);
            }
        }
    }

    fn latency_ms(&self) -> u32 {
        (FRAME as u32 * 1000) / RATE
    }

    fn accepts(&self, rate: u32) -> Result<()> {
        if rate != RATE {
            bail!("{KIND} works at 48000 Hz and the canvas runs at {rate} Hz. Set [canvas] sample_rate = 48000 to use it");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize, level: f32) -> Vec<f32> {
        let mut x: u32 = 12345;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x as f32 / u32::MAX as f32 * 2.0 - 1.0) * level
            })
            .collect()
    }

    fn energy(s: &[f32]) -> f32 {
        s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32
    }

    #[test]
    fn steady_noise_is_taken_well_down() {
        let mut d = Denoise::default();
        d.prepare(RATE, 1);
        // Four seconds of hiss; the network settles in the first second or
        // so, and the last two are what a viewer hears from then on.
        let mut s = noise(RATE as usize * 4, 0.05);
        let tail = RATE as usize * 2;
        let before = energy(&s[tail..]);
        d.process(&mut s, 1);
        let after = energy(&s[tail..]);
        eprintln!("hiss {:.1} dB down", 10.0 * (before / after).log10());
        assert!(after < before * 0.25, "before {before} after {after}");
    }

    #[test]
    fn it_costs_one_frame_and_says_so() {
        let mut d = Denoise::default();
        d.prepare(RATE, 2);
        let mut s = vec![0.5f32; FRAME * 2 - 2];
        d.process(&mut s, 2);
        assert!(s.iter().all(|v| *v == 0.0));
        assert_eq!(d.latency_ms(), 10);
    }

    #[test]
    fn another_rate_is_refused_with_the_setting_to_change() {
        let e = Denoise::default().accepts(44100).unwrap_err().to_string();
        assert!(e.contains("sample_rate = 48000"), "{e}");
    }
}
