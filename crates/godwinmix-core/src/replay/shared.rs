//! What the mixer's replay desk and every replay source both read: the
//! rings, the events and the clock.
//!
//! The desk owns the GStreamer branches and lives on the mixer thread. A
//! replay source is built by the kinds registry, which knows nothing of the
//! desk, so the part both need sits behind one `Arc` that a source finds by
//! the mixer it belongs to (`registry`). Each lock here is held for one
//! lookup or one push, never across anything that waits.

use super::clip::Clip;
use super::marks::Marks;
use super::ring::Ring;
use super::Refusal;
use godwinmix_protocol::replay::{MIN_SPEED, PROGRAMME};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::{Mutex, RwLock};
use std::collections::BTreeMap;
use std::sync::Arc;

/// How far back an unnamed replay starts: ten seconds.
pub const DEFAULT_BACK_SECS: f64 = 10.0;

#[derive(Default)]
pub struct Shared {
    rings: RwLock<BTreeMap<String, Arc<Mutex<Ring>>>>,
    /// The angles in the order they were asked for.
    order: RwLock<Vec<String>>,
    pub marks: Mutex<Marks>,
    clock: RwLock<Option<gst::Clock>>,
}

/// The stretch a play or a save asks for, worked out against the rings.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub from: u64,
    pub to: u64,
    pub angle: String,
    pub event: Option<String>,
}

impl Shared {
    pub fn set_clock(&self, clock: Option<gst::Clock>) {
        *self.clock.write() = clock;
    }

    /// The shared clock now, in nanoseconds.
    pub fn now(&self) -> u64 {
        let clock = self.clock.read().clone().unwrap_or_else(|| gst::SystemClock::obtain());
        clock.time().nseconds()
    }

    pub fn ring(&self, angle: &str) -> Option<Arc<Mutex<Ring>>> {
        self.rings.read().get(angle).cloned()
    }

    pub fn angles(&self) -> Vec<String> {
        self.order.read().clone()
    }

    /// Keep exactly these angles, each `depth_secs` deep. A ring that stays
    /// keeps what it holds.
    pub fn set_angles(&self, angles: &[String], depth_secs: u32) {
        let mut rings = self.rings.write();
        rings.retain(|id, _| angles.contains(id));
        for id in angles {
            rings.entry(id.clone()).or_insert_with(|| Arc::new(Mutex::new(Ring::new(depth_secs)))).lock().set_depth(depth_secs);
        }
        *self.order.write() = angles.to_vec();
    }

    /// The angle a call means: the one it names, then the event's, then the
    /// programme, then the first.
    pub fn pick_angle(&self, asked: Option<&str>, event_angle: Option<&str>) -> Result<String, Refusal> {
        let order = self.angles();
        if order.is_empty() {
            return Err(Refusal::NotInState("replay is off. Turn it on with replay.start and the angles to record".into()));
        }
        let known = |a: &str| order.iter().any(|o| o == a);
        if let Some(a) = asked {
            return if known(a) {
                Ok(a.to_string())
            } else {
                Err(Refusal::NotFound { kind: "angle", id: a.to_string(), known: order.clone() })
            };
        }
        let first = event_angle.filter(|a| known(a)).or_else(|| known(PROGRAMME).then_some(PROGRAMME));
        Ok(first.unwrap_or(&order[0]).to_string())
    }

    /// The stretch an event names, or the last `seconds` before now.
    pub fn window(&self, event: Option<&str>, seconds: Option<f64>, angle: Option<&str>) -> Result<Window, Refusal> {
        let now = self.now();
        let (from, to, event_angle) = match event {
            Some(id) => {
                let marks = self.marks.lock();
                let e = marks.get(id)?;
                (e.in_ms * 1_000_000, e.out_ms.map(|o| o * 1_000_000).unwrap_or(now), e.angle.clone())
            }
            None => {
                let back = seconds.unwrap_or(DEFAULT_BACK_SECS);
                if !back.is_finite() || back <= 0.0 {
                    return Err(Refusal::invalid("seconds", format!("seconds must be more than 0, not {back}")));
                }
                (now.saturating_sub((back * 1e9) as u64), now, None)
            }
        };
        let angle = self.pick_angle(angle, event_angle.as_deref())?;
        Ok(Window { from, to, angle, event: event.map(str::to_string) })
    }

    /// The clip a window holds on one angle, refused when it holds nothing.
    pub fn clip(&self, w: &Window, angle: &str) -> Result<Clip, Refusal> {
        let ring = self.ring(angle).ok_or_else(|| Refusal::NotFound { kind: "angle", id: angle.to_string(), known: self.angles() })?;
        let clip = ring.lock().clip(w.from, w.to);
        if clip.is_empty() {
            return Err(Refusal::NotInState(format!(
                "angle {angle} holds nothing of that stretch yet. It keeps pictures from the moment replay \
                 started on it; wait for its buffer to fill, or pick another angle"
            )));
        }
        Ok(clip)
    }
}

/// A speed a play may run at, or the refusal saying which ones.
pub fn check_speed(speed: Option<f64>) -> Result<f64, Refusal> {
    let s = speed.unwrap_or(1.0);
    if !s.is_finite() || !(MIN_SPEED..=1.0).contains(&s) {
        return Err(Refusal::invalid("speed", format!("speed must be {MIN_SPEED} to 1.0 (1.0 real time, 0.5 half, 0.25 quarter), not {s}")));
    }
    Ok(s)
}
