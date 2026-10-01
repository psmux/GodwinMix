//! One show's alarms, from what has been measured. No media here, only
//! numbers and times, so every rule is tested without a stream.
//!
//! Times are unix milliseconds, handed in, so a test can walk the clock.

use std::collections::{BTreeMap, VecDeque};

use godwinmix_protocol::health::{Alarm, AlarmKind, Health, Hold, Thresholds};

use super::picture::Look;

#[derive(Debug, Default)]
pub struct Judge {
    pub limits: Thresholds,
    live_since: Option<u64>,
    idle_since: u64,
    last_packet: Option<u64>,
    black: Hold,
    freeze: Hold,
    silence: Hold,
    last_look: Option<Look>,
    last_peak: Option<f64>,
    counters: VecDeque<(u64, u64, u64)>,
    failed: BTreeMap<String, (u64, String)>,
}

impl Judge {
    pub fn new(limits: Thresholds, now: u64) -> Judge {
        Judge { limits, idle_since: now, ..Judge::default() }
    }

    /// The input came or went. Going clears everything measured on it.
    pub fn live(&mut self, live: bool, now: u64) {
        match (live, self.live_since) {
            (true, None) => self.live_since = Some(now),
            (false, Some(_)) => {
                *self = Judge { limits: self.limits.clone(), idle_since: now, failed: std::mem::take(&mut self.failed), ..Judge::default() };
            }
            _ => {}
        }
    }

    pub fn packet(&mut self, now: u64) {
        self.last_packet = Some(now);
    }

    /// A picture was looked at. `diff` is its difference from the one before.
    pub fn picture(&mut self, look: Look, diff: Option<f64>, now: u64) {
        let black = look.black_ratio >= self.limits.black_ratio;
        self.black.set(black, now);
        if let Some(d) = diff {
            self.freeze.set(d < self.limits.freeze_diff, now);
        }
        self.last_look = Some(look);
    }

    /// The sound was measured: its peak in dBFS over a short burst.
    pub fn sound(&mut self, peak_db: f64, now: u64) {
        self.silence.set(peak_db < self.limits.silence_db, now);
        self.last_peak = Some(peak_db);
    }

    /// The input's running totals of continuity errors and lost packets.
    pub fn counters(&mut self, cc_errors: u64, lost: u64, now: u64) {
        let window = (self.limits.window_secs * 1000.0) as u64;
        self.counters.push_back((now, cc_errors, lost));
        while self.counters.front().is_some_and(|(t, _, _)| now.saturating_sub(*t) > window) {
            self.counters.pop_front();
        }
    }

    /// An output's state, as the restreamer reports it.
    pub fn output(&mut self, id: &str, failed: Option<&str>, now: u64) {
        match failed {
            Some(why) => {
                self.failed.entry(id.to_string()).or_insert((now, why.to_string()));
            }
            None => {
                self.failed.remove(id);
            }
        }
    }

    /// The black, freeze and silence checks were switched off.
    pub fn clear_media(&mut self) {
        self.black.clear();
        self.freeze.clear();
        self.silence.clear();
    }

    pub fn forget_output(&mut self, id: &str) {
        self.failed.remove(id);
    }

    /// Everything that holds at `now`.
    pub fn health(&self, now: u64) -> Health {
        let mut alarms = Vec::new();
        let mut add = |kind, since_ms, detail: String| alarms.push(Alarm { kind, since_ms, detail });
        for (id, (since, why)) in &self.failed {
            add(AlarmKind::OutputFailed, *since, format!("Output {id} failed: {why}"));
        }
        let Some(live_since) = self.live_since else {
            add(AlarmKind::NoInput, self.idle_since, "Nothing is arriving on the input.".into());
            return Health::from_alarms(alarms);
        };
        let l = &self.limits;
        let last = self.last_packet.unwrap_or(live_since);
        if l.stall_secs > 0.0 && now.saturating_sub(last) >= (l.stall_secs * 1000.0) as u64 {
            let secs = now.saturating_sub(last) / 1000;
            add(AlarmKind::Stall, last, format!("No packet for {secs} s (the limit is {} s).", l.stall_secs));
            return Health::from_alarms(alarms);
        }
        if let Some(since) = self.black.fired(now, l.black_secs) {
            let pct = self.last_look.map(|k| k.black_ratio * 100.0).unwrap_or(100.0);
            add(AlarmKind::Black, since, format!("{pct:.0}% of the picture is black (luma at or under {}).", l.black_luma));
        } else if let Some(since) = self.freeze.fired(now, l.freeze_secs) {
            add(AlarmKind::Freeze, since, format!("The picture has not changed for {} s.", now.saturating_sub(since) / 1000));
        }
        if let Some(since) = self.silence.fired(now, l.silence_secs) {
            let peak = self.last_peak.unwrap_or(f64::NEG_INFINITY);
            add(AlarmKind::Silence, since, format!("Peak {peak:.0} dBFS, under {} dBFS.", l.silence_db));
        }
        self.window_alarms(&mut add);
        Health::from_alarms(alarms)
    }

    fn window_alarms(&self, add: &mut impl FnMut(AlarmKind, u64, String)) {
        let (Some(first), Some(last)) = (self.counters.front(), self.counters.back()) else { return };
        let secs = self.limits.window_secs;
        let cc = last.1.saturating_sub(first.1);
        if self.limits.cc_errors > 0 && cc >= self.limits.cc_errors {
            add(AlarmKind::CcErrors, first.0, format!("{cc} continuity errors in {secs} s."));
        }
        let lost = last.2.saturating_sub(first.2);
        if self.limits.loss > 0 && lost >= self.limits.loss {
            add(AlarmKind::Loss, first.0, format!("{lost} packets lost in {secs} s."));
        }
    }
}

#[cfg(test)]
#[path = "judge_tests.rs"]
mod tests;
