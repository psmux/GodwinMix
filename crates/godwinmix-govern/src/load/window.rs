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
        // The jump the headroom does not already allow for. Other programs
        // are taken off at their peak and this process at its average, so
        // only the part of the machine's peak above those two is a burst
        // still to keep room for. Peak less mean, as this was, counted a
        // spiking browser twice: once in the peak of other programs and
        // again in the reserve. On a sixteen core desktop at 40 to 50% with
        // the odd spike to 70%, the two together came to more than the
        // machine and a three rung ladder was refused with "0.0 cores is
        // free".
        let burst = f64::from(peak) - f64::from(others_peak) - self.own_avg;
        let last = self.readings.back();
        Load {
            system_millicores: self.system_avg.round() as u32,
            own_millicores: self.own_avg.round() as u32,
            others_peak_millicores: others_peak,
            jitter_millicores: burst.max(0.0).round() as u32,
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
    fn a_show_that_dies_leaves_no_other_programs_peak_behind() {
        // The shows' own load used to be taken off when the window was read.
        // A show killed a second ago was still in the last ten readings, the
        // shows now measured nothing, and its whole load read as another
        // program's peak for ten seconds: the same show, started again, was
        // refused its rendition with "0.0 cores is free".
        let mut w = Window::new(10);
        for _ in 0..5 {
            w.push(r(3100, 100).with_elsewhere(3000));
        }
        let after = w.push(r(100, 100).with_elsewhere(0));
        assert_eq!(after.others_peak_millicores, 0, "{after:?}");
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
    }

    #[test]
    fn another_programs_spike_is_counted_once_in_its_peak_not_again_as_jitter() {
        // Sixteen cores, 40 to 50% busy, a browser that spikes to 70% every
        // few seconds, this process steady at half a core.
        let mut w = Window::new(10);
        let mut l = w.push(r(7000, 500));
        for system in [6500, 11_500, 7000, 6400, 8000, 7200, 11_000, 6600, 7000] {
            l = w.push(r(system, 500));
        }
        assert_eq!(l.others_peak_millicores, 11_000, "{l:?}");
        assert_eq!(l.jitter_millicores, 0, "the spike is in the peak of other programs already: {l:?}");
    }

    #[test]
    fn a_burst_of_this_process_above_its_average_is_still_jitter() {
        // Other programs steady; this process jumps from half a core to three
        // for one second, which its moving average does not show yet.
        let mut w = Window::new(10);
        let mut l = w.push(r(2500, 500));
        for own in [500, 500, 3000, 500, 500] {
            l = w.push(r(2000 + own, own));
        }
        assert_eq!(l.others_peak_millicores, 2000, "{l:?}");
        assert!(l.jitter_millicores > 1500, "{l:?}");
        assert!(l.jitter_millicores < 2500, "{l:?}");
    }
}
