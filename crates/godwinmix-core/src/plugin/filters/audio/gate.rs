//! `gate/filter`: silence between words.
//!
//! The gate opens when the level passes `open_threshold_db` and starts to
//! close only once it has stayed under `close_threshold_db` for `hold_ms`,
//! so a pause for breath does not chop the next word. Opening takes
//! `attack_ms` and closing `release_ms`, as straight ramps, down to
//! `floor_db` (silence by default). OBS writes `open_threshold`,
//! `close_threshold`, `attack_time`, `hold_time` and `release_time`.

use super::run::Dsp;
use super::values::{db_to_gain, num};
use crate::config::Params;
use anyhow::{bail, Result};

const KIND: &str = "gate/filter";

#[derive(Debug, Clone, Copy, PartialEq)]
struct Settings {
    open: f32,
    close: f32,
    attack_ms: f64,
    hold_ms: f64,
    release_ms: f64,
    floor: f32,
}

pub struct Gate {
    s: Settings,
    rate: u32,
    gain: f32,
    /// Frames left before a gate under its close level starts to close.
    hold_left: u32,
    open: bool,
}

impl Default for Gate {
    fn default() -> Self {
        let s = Settings { open: db_to_gain(-26.0), close: db_to_gain(-32.0), attack_ms: 25.0, hold_ms: 200.0, release_ms: 150.0, floor: 0.0 };
        Gate { s, rate: 48000, gain: 0.0, hold_left: 0, open: false }
    }
}

impl Dsp for Gate {
    fn set(&mut self, params: &Params) -> Result<()> {
        let open = num(KIND, params, &["open_threshold_db", "open_threshold"], -26.0, -96.0, 0.0)?;
        let close = num(KIND, params, &["close_threshold_db", "close_threshold"], -32.0, -96.0, 0.0)?;
        if close > open {
            bail!("{KIND} params.close_threshold_db ({close}) must not be above open_threshold_db ({open}), or the gate would flutter");
        }
        let floor = num(KIND, params, &["floor_db"], -96.0, -96.0, 0.0)?;
        self.s = Settings {
            open: db_to_gain(open),
            close: db_to_gain(close),
            attack_ms: num(KIND, params, &["attack_ms", "attack_time"], 25.0, 0.0, 500.0)?,
            hold_ms: num(KIND, params, &["hold_ms", "hold_time"], 200.0, 0.0, 2000.0)?,
            release_ms: num(KIND, params, &["release_ms", "release_time"], 150.0, 0.0, 2000.0)?,
            floor: if floor <= -96.0 { 0.0 } else { db_to_gain(floor) },
        };
        Ok(())
    }

    fn prepare(&mut self, rate: u32, _channels: usize) {
        self.rate = rate;
    }

    fn process(&mut self, samples: &mut [f32], channels: usize) {
        let per_ms = self.rate as f64 / 1000.0;
        let step = |ms: f64| if ms <= 0.0 { 1.0 } else { (1.0 / (ms * per_ms)) as f32 };
        let (up, down) = (step(self.s.attack_ms), step(self.s.release_ms));
        let hold = (self.s.hold_ms * per_ms) as u32;
        for frame in samples.chunks_exact_mut(channels) {
            let peak = frame.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            if peak >= self.s.open {
                self.open = true;
                self.hold_left = hold;
            } else if peak < self.s.close && self.open {
                if self.hold_left == 0 {
                    self.open = false;
                } else {
                    self.hold_left -= 1;
                }
            }
            self.gain = if self.open { (self.gain + up).min(1.0) } else { (self.gain - down).max(0.0) };
            let g = self.s.floor + (1.0 - self.s.floor) * self.gain;
            frame.iter_mut().for_each(|x| *x *= g);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> Gate {
        let mut g = Gate::default();
        g.prepare(48000, 1);
        g.set(&toml::from_str("attack_ms = 0\nhold_ms = 10\nrelease_ms = 5").unwrap()).unwrap();
        g
    }

    #[test]
    fn the_room_between_words_is_silenced() {
        let mut g = gate();
        let mut room = vec![0.005f32; 48000];
        g.process(&mut room, 1);
        assert!(room[1000..].iter().all(|v| *v == 0.0));
    }

    #[test]
    fn a_word_goes_through_and_its_tail_is_held() {
        let mut g = gate();
        let mut s = vec![0.5f32; 4800];
        s.extend(vec![0.01f32; 4800]);
        g.process(&mut s, 1);
        assert!((s[100] - 0.5).abs() < 1e-6);
        // 5 ms into the quiet: inside the hold, still open.
        assert!((s[4800 + 240] - 0.01).abs() < 1e-6);
        // 30 ms in: held 10, closed over 5.
        assert_eq!(s[4800 + 1440], 0.0);
    }

    #[test]
    fn a_close_level_above_the_open_level_is_refused() {
        let e = Gate::default()
            .set(&toml::from_str("open_threshold_db = -40\nclose_threshold_db = -20").unwrap())
            .unwrap_err()
            .to_string();
        assert!(e.contains("flutter"), "{e}");
    }
}
