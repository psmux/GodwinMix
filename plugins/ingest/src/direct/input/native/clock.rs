//! One program's clock: its PCR and every PES time on one line, in
//! nanoseconds, unwrapped across the 33 bit wrap, and carried on across a
//! sender that restarts with its clock somewhere else.
//!
//! Every time comes from the stream itself, never from when it arrived.
//! Times are counted from the program's first PCR, as `tsdemux` counts
//! them, so each one rounds to the same millisecond the GStreamer path
//! gives it. When the PCR jumps (back at all, or ahead by more than
//! [`AHEAD`], or flagged as a discontinuity) the new clock is laid a frame
//! after the old one, and every stream moves by the same amount, so sound
//! and picture keep their distance.

use super::es::rate::ns_of_90k;
use super::es::Ns;

const WRAP: i64 = 1 << 33;
/// Times are handed on this far up, a whole number of milliseconds, so a
/// time a little before the first PCR is still a positive number.
const UP: i64 = 1_000_000_000_000_000;
/// A PCR this far ahead of the last one is a new clock: PCRs come at least
/// ten times a second, so this is a sender that went away and came back.
const AHEAD: i64 = 3 * 90_000;
/// How far after the old clock's last PCR a new clock starts.
const GAP: i64 = 3_600;
/// A PES time this far before the last PCR, or [`LATER`] after it, belongs
/// to a clock the PCR has not caught up with yet: a sender that restarted,
/// whose first sound came before its first PCR. It is dropped rather than
/// laid on the old clock, where it would land seconds away.
const EARLIER: i64 = 90_000;
const LATER: i64 = 5 * 90_000;

#[derive(Debug, Default)]
pub struct Clock {
    /// The last time seen, unwrapped, in 90 kHz ticks.
    last: Option<i64>,
    /// Added to every time since the last new clock.
    shift: i64,
    last_pcr: Option<i64>,
    /// The first PCR, in nanoseconds.
    base: Option<i64>,
}

impl Clock {
    fn unwrap(&mut self, t: u64) -> i64 {
        let t = t as i64 & (WRAP - 1);
        let v = match self.last {
            None => t + WRAP,
            Some(l) => {
                let mut d = t - l.rem_euclid(WRAP);
                if d > WRAP / 2 {
                    d -= WRAP;
                } else if d < -WRAP / 2 {
                    d += WRAP;
                }
                l + d
            }
        };
        self.last = Some(v);
        v
    }

    /// A PCR: its 33 bit base, its 27 MHz extension, and whether the
    /// packet said the clock jumps here.
    pub fn pcr(&mut self, base: u64, ext: u16, discontinuity: bool) {
        let v = self.unwrap(base);
        if let Some(lp) = self.last_pcr {
            let d = v + self.shift - lp;
            if discontinuity || !(0..=AHEAD).contains(&d) {
                self.shift = lp + GAP - v;
            }
        }
        let at = v + self.shift;
        self.last_pcr = Some(at);
        self.base.get_or_insert((at * 300 + i64::from(ext)) * 1000 / 27);
    }

    /// A program with no PCR counts from its first PES time instead.
    pub fn no_pcr(&mut self, first: u64) {
        if self.base.is_none() {
            let v = self.unwrap(first);
            self.base = Some(ns_of_90k(v as u64) as i64);
        }
    }

    /// A PES time on the program's line, or `None` before the first PCR
    /// or on a clock the PCR does not speak for.
    pub fn time(&mut self, t: u64) -> Option<Ns> {
        let base = self.base?;
        let v = self.unwrap(t) + self.shift;
        if self.last_pcr.is_some_and(|lp| v < lp - EARLIER || v > lp + LATER) {
            return None;
        }
        Some((ns_of_90k(v.max(0) as u64) as i64 - base + UP).max(0) as Ns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Equal to the nanosecond either way: two times each rounded down.
    fn near(a: u64, b: u64) -> bool {
        a.abs_diff(b) <= 1
    }

    #[test]
    fn times_count_from_the_first_pcr_and_go_on_across_the_wrap() {
        let mut c = Clock::default();
        assert_eq!(c.time(1000), None, "nothing before a PCR");
        let start = (WRAP - 90_000) as u64;
        c.pcr(start, 0, false);
        assert!(near(c.time(start + 63_000).unwrap() - UP as u64, 700_000_000));
        // Two seconds on, the 33 bit time has wrapped past zero.
        c.pcr(90_000, 0, false);
        assert!(near(c.time(90_000 + 63_000).unwrap() - UP as u64, 2_700_000_000));
    }

    #[test]
    fn a_sender_with_a_new_clock_carries_on_and_keeps_sound_with_picture() {
        let mut c = Clock::default();
        c.pcr(900_000, 0, false);
        let before = c.time(900_000 + 9_000).unwrap();
        // The sender came back an hour earlier on its own clock.
        c.pcr(100_000, 0, false);
        let video = c.time(100_000 + 9_000).unwrap();
        let audio = c.time(100_000 + 9_000 + 2_700).unwrap();
        assert!(video > before && video - before < 100_000_000, "{before} then {video}");
        assert!(near(audio - video, 30_000_000), "30 ms apart before the jump and after");
        // Sound from the next restart, ahead of that sender's first PCR.
        assert_eq!(c.time(5_000_000), None);
    }
}
