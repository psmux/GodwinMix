//! `delay/filter`: hold a source's sound back to meet its picture.
//!
//! A camera on HDMI capture or over the network is often later than the
//! microphone beside the presenter. `delay_ms` delays the sound by that much
//! with a ring of samples, leaving the timestamps alone, so the mixer and the
//! encoder see no change in timing. A change of delay on air starts the ring
//! again from silence, which is a short gap and never a pile of old sound.

use super::run::Dsp;
use super::values::num;
use crate::config::Params;
use anyhow::Result;

const KIND: &str = "delay/filter";
pub const MAX_MS: f64 = 2000.0;

pub struct Delay {
    ms: f64,
    ring: Vec<f32>,
    at: usize,
    format: (u32, usize),
}

impl Default for Delay {
    fn default() -> Self {
        Delay { ms: 0.0, ring: Vec::new(), at: 0, format: (48000, 2) }
    }
}

impl Delay {
    fn resize(&mut self) {
        let (rate, channels) = self.format;
        let frames = (self.ms * rate as f64 / 1000.0).round() as usize;
        self.ring = vec![0.0; frames * channels];
        self.at = 0;
    }
}

impl Dsp for Delay {
    fn set(&mut self, params: &Params) -> Result<()> {
        let ms = num(KIND, params, &["delay_ms", "ms"], 0.0, 0.0, MAX_MS)?;
        if ms != self.ms {
            self.ms = ms;
            self.resize();
        }
        Ok(())
    }

    fn prepare(&mut self, rate: u32, channels: usize) {
        self.format = (rate, channels);
        self.resize();
    }

    fn process(&mut self, samples: &mut [f32], _channels: usize) {
        if self.ring.is_empty() {
            return;
        }
        let len = self.ring.len();
        for s in samples.iter_mut() {
            std::mem::swap(s, &mut self.ring[self.at]);
            self.at += 1;
            if self.at == len {
                self.at = 0;
            }
        }
    }

    fn latency_ms(&self) -> u32 {
        self.ms as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sound_comes_out_exactly_that_much_later() {
        let mut d = Delay::default();
        d.prepare(1000, 2);
        d.set(&toml::from_str("delay_ms = 3").unwrap()).unwrap();
        let mut s: Vec<f32> = (1..=10).map(|n| n as f32).collect();
        d.process(&mut s, 2);
        assert_eq!(s, vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0]);
        let mut next = vec![0.0; 6];
        d.process(&mut next, 2);
        assert_eq!(next, vec![5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
    }

    #[test]
    fn no_delay_passes_the_sound_untouched() {
        let mut d = Delay::default();
        d.prepare(48000, 2);
        let mut s = vec![0.25f32; 8];
        d.process(&mut s, 2);
        assert_eq!(s, vec![0.25; 8]);
    }
}
