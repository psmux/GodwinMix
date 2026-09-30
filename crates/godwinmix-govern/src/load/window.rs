//! Smoothing: a moving average for what is running, and the recent peak of
//! everything that is not this process.
//!
//! The peak rather than the average for other processes, because a browser
//! tab or a backup that spikes every few seconds will spike again, and the
//! room it needs is the room at its peak.

use super::{Load, Reading};
use std::collections::VecDeque;

pub struct Window {
    size: usize,
    readings: VecDeque<Reading>,
    system_avg: f64,
    own_avg: f64,
    samples: u64,
}

impl Window {
    pub fn new(size: usize) -> Window {
        Window { size: size.max(1), readings: VecDeque::new(), system_avg: 0.0, own_avg: 0.0, samples: 0 }
    }

    /// Take one reading in and say what the load is now.
    pub fn push(&mut self, r: Reading) -> Load {
        // Weight 0.3: a real change shows within three or four seconds, one
        // odd second moves the figure by less than a third of its size.
        const A: f64 = 0.3;
        if self.samples == 0 {
            self.system_avg = f64::from(r.system_millicores);
            self.own_avg = f64::from(r.own_millicores);
        } else {
            self.system_avg += A * (f64::from(r.system_millicores) - self.system_avg);
            self.own_avg += A * (f64::from(r.own_millicores) - self.own_avg);
        }
        self.samples += 1;
        if self.readings.len() == self.size {
            self.readings.pop_front();
        }
        self.readings.push_back(r);
        self.load()
    }

    fn load(&self) -> Load {
        let others = |r: &Reading| r.system_millicores.saturating_sub(r.own_millicores);
        let others_peak = self.readings.iter().map(others).max().unwrap_or(0);
        let peak = self.readings.iter().map(|r| r.system_millicores).max().unwrap_or(0);
        let mean = self.readings.iter().map(|r| f64::from(r.system_millicores)).sum::<f64>()
            / self.readings.len().max(1) as f64;
        let last = self.readings.back();
        Load {
            system_millicores: self.system_avg.round() as u32,
            own_millicores: self.own_avg.round() as u32,
            others_peak_millicores: others_peak,
            jitter_millicores: (f64::from(peak) - mean).max(0.0).round() as u32,
            available_mib: last.and_then(|r| r.available_mib),
            devices: last.map(|r| r.devices.clone()).unwrap_or_default(),
            samples: self.samples,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(system: u32, own: u32) -> Reading {
        Reading { system_millicores: system, own_millicores: own, ..Default::default() }
    }

    #[test]
    fn the_peak_of_other_processes_is_kept_for_the_window() {
        let mut w = Window::new(3);
        w.push(r(1000, 200));
        w.push(r(4000, 200));
        let l = w.push(r(1000, 200));
        assert_eq!(l.others_peak_millicores, 3800);
        w.push(r(1000, 200));
        let l = w.push(r(1000, 200));
        assert_eq!(l.others_peak_millicores, 800, "the spike has left a window of three");
    }

    #[test]
    fn the_average_follows_a_real_change_within_a_few_seconds() {
        let mut w = Window::new(10);
        w.push(r(1000, 0));
        let mut l = w.push(r(3000, 0));
        for _ in 0..5 {
            l = w.push(r(3000, 0));
        }
        assert!(l.system_millicores > 2600, "{l:?}");
        assert!(l.jitter_millicores > 0);
    }
}
