//! Every watched show, the one thread that reads them, and what the direct
//! host calls.
//!
//! One thread drains every show's hub reader each `TURN` and judges every
//! show once a second. Nothing it does waits on a decode: the workers are
//! behind a queue that drops when full.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use godwinmix_protocol::health::{Health, Thresholds};
use gstreamer as gst;
use serde_json::{json, Value};

use super::show::{lock, Show, Thumb};
use super::tap::{Tap, PACE};
use super::work::{Job, Pool};
use super::now_ms;
use crate::hub::Hub;

/// How often the hub readers are drained.
const TURN: Duration = Duration::from_millis(20);
/// How long one thumbnail request keeps a show's pictures coming.
pub const ASKED_FOR_MS: u64 = 10_000;

/// Where `event/direct.health` goes: the plugin's reporter, or a test.
pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

pub struct Vitals {
    hub: Hub,
    pool: Pool,
    taps: Mutex<BTreeMap<String, Tap>>,
    emit: Emit,
    stopped: AtomicBool,
}

impl Vitals {
    /// Start the reading thread and `workers` decoding threads. Both stop
    /// when the last handle goes.
    pub fn start(hub: Hub, emit: Emit, workers: usize) -> Arc<Vitals> {
        let pool = Pool::start(workers, 64);
        let me = Arc::new(Vitals { hub, pool, taps: Mutex::default(), emit, stopped: AtomicBool::new(false) });
        let weak = Arc::downgrade(&me);
        std::thread::Builder::new().name("vitals-tap".into()).spawn(move || run(weak)).expect("the vitals thread");
        me
    }

    /// Watch a show's input, `stream` in `app` on the hub, as a direct
    /// table row's `monitor` asks: `{alarms, pictures, thresholds}`. Called
    /// again with a changed row, it changes the show in place.
    pub fn watch(&self, id: &str, app: &str, stream: &str, monitor: &Value) {
        let limits: Thresholds = serde_json::from_value(monitor["thresholds"].clone()).unwrap_or_default();
        let mut taps = lock(&self.taps);
        let moved = taps.get(id).is_some_and(|t| t.app != app || t.stream != stream);
        if moved || !taps.contains_key(id) {
            let show = Arc::new(Show::new(id, limits.clone(), now_ms()));
            taps.insert(id.to_string(), Tap::new(show, app, stream));
        }
        let show = &taps[id].show;
        show.alarms.store(monitor["alarms"].as_bool().unwrap_or(true), Ordering::Relaxed);
        show.pictures.store(monitor["pictures"].as_bool().unwrap_or(false), Ordering::Relaxed);
        lock(&show.judge).limits = limits;
    }

    /// Stop watching every show not in `ids`.
    pub fn keep(&self, ids: &[&str]) {
        lock(&self.taps).retain(|id, _| ids.contains(&id.as_str()));
    }

    fn show(&self, id: &str) -> Option<Arc<Show>> {
        lock(&self.taps).get(id).map(|t| t.show.clone())
    }

    /// The input's running totals of continuity errors and lost packets.
    pub fn counters(&self, id: &str, cc_errors: u64, lost: u64) {
        if let Some(s) = self.show(id) {
            lock(&s.judge).counters(cc_errors, lost, now_ms());
        }
    }

    /// An output's state. `failed` carries the error while it is failed.
    pub fn output(&self, id: &str, output: &str, failed: Option<&str>) {
        if let Some(s) = self.show(id) {
            lock(&s.judge).output(output, failed, now_ms());
        }
    }

    /// A frame from a decode the show already runs (a rendition). Taken at
    /// most once a `PACE`, and while it keeps coming no keyframe is decoded.
    pub fn offer_frame(&self, id: &str, sample: &gst::Sample) {
        let now = now_ms();
        let mut taps = lock(&self.taps);
        let Some(tap) = taps.get_mut(id) else { return };
        if now.saturating_sub(tap.offered_at) + 100 < PACE || !tap.show.wants_pictures(now) {
            return;
        }
        tap.offered_at = now;
        let (Some(caps), Some(buffer)) = (sample.caps_owned(), sample.buffer_owned()) else { return };
        let jpeg = tap.show.wants_jpeg(now);
        self.pool.offer(Job::Picture { show: tap.show.clone(), caps, buffer, jpeg });
    }

    /// The newest thumbnail, and pictures kept on for `ASKED_FOR_MS` more.
    /// `Err` for a show this host does not run; `Ok(None)` while the first
    /// keyframe is still on its way.
    pub fn thumbnail(&self, id: &str) -> Result<Option<Thumb>, String> {
        let show = self.show(id).ok_or_else(|| format!("no direct show called {id} is running here"))?;
        show.asked_until.store(now_ms() + ASKED_FOR_MS, Ordering::Relaxed);
        let thumb = lock(&show.thumb).clone();
        Ok(thumb)
    }

    /// The show's health as last judged.
    pub fn health(&self, id: &str) -> Option<Health> {
        let show = self.show(id)?;
        let h = lock(&show.reported).clone();
        h
    }

    /// Jobs dropped because every worker was busy, and jobs done.
    pub fn counts(&self) -> (u64, u64) {
        (self.pool.skipped.load(Ordering::Relaxed), self.pool.done.load(Ordering::Relaxed))
    }

    /// Judge every show, and say so for each whose health changed.
    fn tick(&self, now: u64) {
        let shows: Vec<(Arc<Show>, bool)> = {
            let taps = lock(&self.taps);
            taps.values().map(|t| (t.show.clone(), self.hub.is_live(&t.app, &t.stream))).collect()
        };
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

impl Drop for Vitals {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}

fn run(weak: Weak<Vitals>) {
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
