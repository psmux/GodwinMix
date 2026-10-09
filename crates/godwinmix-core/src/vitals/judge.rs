//! A compositing show's alarms, from the programme's numbers. No media here,
//! so every rule is tested without a pipeline. Times are unix milliseconds.

use std::collections::BTreeMap;

use godwinmix_protocol::health::{Alarm, AlarmKind, Health, Hold, Thresholds};

use crate::state::{OutputState, OutputStatus};

#[derive(Debug, Default)]
pub struct Judge {
    pub limits: Thresholds,
    black: Hold,
    freeze: Hold,
    silence: Hold,
    last_black: f64,
    last_peak: Option<f64>,
    /// Outputs that are failed, or shed by the governor, with since and why.
    failed: BTreeMap<String, (u64, String)>,
    shed: BTreeMap<String, (u64, String)>,
    /// The memory guard's alarm while it holds: since, and the sentence.
    memory: Option<(u64, String)>,
}

impl Judge {
    pub fn new(limits: Thresholds) -> Judge {
        Judge { limits, ..Judge::default() }
    }

    /// The programme cell, looked at: its share of black pixels and its
    /// motion since the mosaic frame before, both 0 to 1.
    pub fn picture(&mut self, black_ratio: f64, motion: Option<f64>, now: u64) {
        self.black.set(black_ratio >= self.limits.black_ratio, now);
        if let Some(m) = motion {
            self.freeze.set(m < self.limits.freeze_diff, now);
        }
        self.last_black = black_ratio;
    }

    /// The alarms are off, or there is no mosaic to look at: stop judging
    /// the picture.
    pub fn no_picture(&mut self) {
        self.black.clear();
        self.freeze.clear();
    }

    /// The alarms are off, or nothing with sound is on programme: there is
    /// no sound to judge, so it is not silent.
    pub fn no_sound(&mut self) {
        self.silence.clear();
        self.last_peak = None;
    }

    /// The programme meter's loudest channel, in dBFS.
    pub fn sound(&mut self, peak_db: f64, now: u64) {
        self.silence.set(peak_db < self.limits.silence_db, now);
        self.last_peak = Some(peak_db);
    }

    /// Every output, as the status reports them.
    pub fn outputs(&mut self, outputs: &[OutputStatus], now: u64) {
        let failed = outputs.iter().filter(|o| o.state == OutputState::Failed).map(|o| (o.id.clone(), "it stopped trying".to_string()));
        keep(&mut self.failed, failed.collect(), now);
        let shed = outputs.iter().filter_map(|o| o.shed.clone().map(|why| (o.id.clone(), why)));
        keep(&mut self.shed, shed.collect(), now);
    }

    /// The show's memory guard, as `mixer::memguard::pressure` says now.
    pub fn memory(&mut self, pressure: Option<(u64, String)>) {
        self.memory = pressure;
    }

    pub fn health(&self, now: u64) -> Health {
        let l = &self.limits;
        let mut alarms = Vec::new();
        let mut add = |kind, since_ms, detail: String| alarms.push(Alarm { kind, since_ms, detail });
        if let Some(since) = self.black.fired(now, l.black_secs) {
            add(AlarmKind::Black, since, format!("{:.0}% of the programme is black.", self.last_black * 100.0));
        } else if let Some(since) = self.freeze.fired(now, l.freeze_secs) {
            add(AlarmKind::Freeze, since, format!("The programme has not moved for {} s.", now.saturating_sub(since) / 1000));
        }
        if let Some(since) = self.silence.fired(now, l.silence_secs) {
            let peak = self.last_peak.unwrap_or(f64::NEG_INFINITY);
            add(AlarmKind::Silence, since, format!("Programme peak {peak:.0} dBFS, under {} dBFS.", l.silence_db));
        }
        for (id, (since, why)) in &self.failed {
            add(AlarmKind::OutputFailed, *since, format!("Output {id} failed: {why}."));
        }
        for (id, (since, why)) in &self.shed {
            add(AlarmKind::Shed, *since, format!("Output {id} is shed: {why}"));
        }
        if let Some((since, detail)) = &self.memory {
            add(AlarmKind::Memory, *since, detail.clone());
        }
        Health::from_alarms(alarms)
    }
}

/// Keep the entries still in `now_in`, with the time they first appeared.
fn keep(map: &mut BTreeMap<String, (u64, String)>, now_in: BTreeMap<String, String>, now: u64) {
    map.retain(|id, _| now_in.contains_key(id));
    for (id, why) in now_in {
        map.entry(id).or_insert((now, why));
    }
}

#[cfg(test)]
#[path = "judge_tests.rs"]
mod tests;
