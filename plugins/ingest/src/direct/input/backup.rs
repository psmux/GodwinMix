//! A main input with a backup: both run all the time, so the backup is warm
//! when it is needed and the main is known to be back the moment it is.
//!
//! The switch to the backup happens when the main has sent no frame for
//! `stall_ms` (default 2 s), at the backup's next keyframe. The switch back
//! happens when the main has been steady for `return_ms` (default 5 s), at
//! the main's next keyframe. Each switch sends the incoming side's codec
//! headers first and lays its timeline after the last tag sent, so a reader
//! sees one stream that never goes backwards.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[path = "backup_switch.rs"]
mod switch;
use switch::Switch;

use super::super::{Input, Sink, StopSignal, TagSink};
use super::spec::InputSpec;
use super::stats::InputStats;
use crate::media_tag::MediaTag;

pub struct Backup {
    main: Box<dyn Input>,
    backup: Box<dyn Input>,
    stall_ms: u64,
    return_ms: u64,
}

impl Backup {
    pub fn new(main: Box<dyn Input>, backup: Box<dyn Input>, spec: &InputSpec) -> Backup {
        let stall_ms = spec.number("stall_ms").unwrap_or(2_000).clamp(250, 60_000);
        let return_ms = spec.number("return_ms").unwrap_or(5_000).clamp(0, 600_000);
        Backup { main, backup, stall_ms, return_ms }
    }
}

struct Side(Arc<Mutex<Switch>>, usize);

impl Side {
    fn lock(&self) -> MutexGuard<'_, Switch> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl TagSink for Side {
    fn tag(&mut self, tag: MediaTag) {
        self.lock().tag(self.1, tag);
    }

    fn stats(&mut self, stats: &InputStats) {
        self.lock().stats(self.1, stats);
    }
}

impl Input for Backup {
    fn run(self: Box<Self>, out: Sink, stop: StopSignal) {
        let switch = Switch { out, legs: Default::default(), active: 0, want: 0, end_ms: 0, started: Instant::now() };
        let switch = Arc::new(Mutex::new(switch));
        let (stall, settle) = (Duration::from_millis(self.stall_ms), Duration::from_millis(self.return_ms));
        let Backup { main, backup, .. } = *self;
        let threads: Vec<_> = [main, backup]
            .into_iter()
            .enumerate()
            .map(|(i, input)| {
                let (side, stop) = (Box::new(Side(switch.clone(), i)), stop.clone());
                std::thread::Builder::new().name(format!("direct-in-{}", ["main", "backup"][i])).spawn(move || input.run(side, stop))
            })
            .collect();
        while !stop.wait(Duration::from_millis(250)) {
            switch.lock().unwrap_or_else(|e| e.into_inner()).decide(stall, settle);
        }
        for t in threads.into_iter().flatten() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
