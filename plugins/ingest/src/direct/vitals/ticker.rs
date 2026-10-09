//! The one thread: it drains every show's hub reader each `TURN` and judges
//! every show once a second. Nothing it does waits on a decode: the workers
//! are behind a queue that drops when full.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::Duration;

use serde_json::json;

use super::now_ms;
use super::registry::Vitals;
use super::show::{lock, Show};

/// How often the hub readers are drained.
const TURN: Duration = Duration::from_millis(20);

impl Vitals {
    /// Judge every show, and say so for each whose health changed.
    pub(super) fn tick(&self, now: u64) {
        let shows: Vec<(Arc<Show>, bool)> = {
            let taps = lock(&self.taps);
            taps.values().filter(|t| !t.peek).map(|t| (t.show.clone(), self.hub.is_live(&t.app, &t.stream))).collect()
        };
        self.forget_peeks(now);
        for (show, live) in shows {
            let health = {
                let mut judge = lock(&show.judge);
                judge.live(live, now);
                let last = show.last_packet.load(Ordering::Relaxed);
                if last > 0 {
                    judge.packet(last);
                }
                if !show.alarms.load(Ordering::Relaxed) {
                    judge.clear_media();
                }
                judge.health(now)
            };
            show.forget_pictures(now);
            let mut reported = lock(&show.reported);
            if reported.as_ref().is_none_or(|r| health.changed_from(r)) {
                (self.emit)("direct.health", json!({"show": show.id, "health": health}));
            }
            *reported = Some(health);
        }
    }
}

pub fn run(weak: Weak<Vitals>) {
    let mut next_tick = 0;
    while let Some(v) = weak.upgrade() {
        if v.stopped.load(Ordering::Relaxed) {
            return;
        }
        let now = now_ms();
        for tap in lock(&v.taps).values_mut() {
            tap.drain(&v.hub, &v.pool, now);
        }
        if now >= next_tick {
            v.tick(now);
            next_tick = now + 1000;
        }
        drop(v);
        std::thread::sleep(TURN);
    }
}
