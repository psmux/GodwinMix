//! One show's vitals: its judge, the last picture, and who wants what.
//!
//! Shared between the tap (which hands its keyframes and sound to the
//! workers), the workers (which write what they measured) and the ticker
//! (which judges once a second). Every lock here is held for a few field
//! writes and never across a decode.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use godwinmix_protocol::health::{Health, Thresholds};

use super::judge::Judge;
use super::picture::Luma;

/// A thumbnail, ready to serve.
#[derive(Debug, Clone)]
pub struct Thumb {
    pub jpeg: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    /// Unix ms of the keyframe it was decoded from.
    pub at_ms: u64,
}

pub struct Show {
    pub id: String,
    pub judge: Mutex<Judge>,
    /// The last picture's luma, for the next freeze comparison.
    pub luma: Mutex<Option<Luma>>,
    pub thumb: Mutex<Option<Thumb>>,
    /// The black, freeze and silence checks: decode keyframes and some sound.
    pub alarms: AtomicBool,
    /// The station says someone is looking (`monitor.pictures`).
    pub pictures: AtomicBool,
    /// Unix ms until which a thumbnail request keeps pictures on.
    pub asked_until: AtomicU64,
    /// Unix ms of the last packet the tap saw.
    pub last_packet: AtomicU64,
    /// The health last reported, so only a change is sent.
    pub reported: Mutex<Option<Health>>,
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Show {
    pub fn new(id: &str, limits: Thresholds, now: u64) -> Show {
        Show {
            id: id.to_string(),
            judge: Mutex::new(Judge::new(limits, now)),
            luma: Mutex::new(None),
            thumb: Mutex::new(None),
            alarms: AtomicBool::new(true),
            pictures: AtomicBool::new(false),
            asked_until: AtomicU64::new(0),
            last_packet: AtomicU64::new(0),
            reported: Mutex::new(None),
        }
    }

    /// Whether anyone wants a JPEG made from the next keyframe.
    pub fn wants_jpeg(&self, now: u64) -> bool {
        self.pictures.load(Ordering::Relaxed) || self.asked_until.load(Ordering::Relaxed) > now
    }

    /// Whether the next keyframe should be decoded at all.
    pub fn wants_pictures(&self, now: u64) -> bool {
        self.alarms.load(Ordering::Relaxed) || self.wants_jpeg(now)
    }

    /// Whether some sound should be decoded: only for the silence check.
    pub fn wants_sound(&self) -> bool {
        self.alarms.load(Ordering::Relaxed)
    }

    /// A new picture: judged, and kept for the next comparison.
    pub fn picture(&self, luma: Luma, now: u64) {
        let mut last = lock(&self.luma);
        let diff = last.as_ref().and_then(|before| luma.diff(before));
        let mut judge = lock(&self.judge);
        let look = luma.look(judge.limits.black_luma);
        judge.picture(look, diff, now);
        *last = Some(luma);
    }

    /// Nobody wants pictures and no alarm needs them: let the memory go.
    pub fn forget_pictures(&self, now: u64) {
        if !self.wants_jpeg(now) {
            lock(&self.thumb).take();
        }
        if !self.wants_pictures(now) {
            lock(&self.luma).take();
        }
    }
}
