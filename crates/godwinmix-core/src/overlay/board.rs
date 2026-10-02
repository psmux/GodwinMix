//! The board: every transparent source on this programme, and the one pad
//! probe that draws them.
//!
//! The probe sits on the compositor's src pad and is there only while at
//! least one transparent source exists. With none it is not installed at all,
//! so a show with no text, ticker or transparent picture runs exactly the
//! graph it ran before this module.

use super::picture::{Layer, Motion};
use super::place::{self, PadBox};
use super::draw::{Job, Jobs};
use crate::plugin::branch::VideoPads;
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

/// Where a crawl has got to, kept per source so a change of speed carries on
/// from the same place instead of jumping.
#[derive(Default)]
struct Clock {
    epoch: Option<u64>,
    since: u64,
    base: f64,
    speed: f64,
}

impl Clock {
    /// Pixels travelled at `now` (nanoseconds of running time).
    fn travelled(&mut self, now: u64, epoch: u64, speed: f64) -> f64 {
        if self.epoch != Some(epoch) {
            *self = Clock { epoch: Some(epoch), since: now, base: 0.0, speed };
        }
        let elapsed = now.saturating_sub(self.since) as f64 / 1e9;
        if (speed - self.speed).abs() > f64::EPSILON {
            self.base += elapsed * self.speed;
            self.since = now;
            self.speed = speed;
            return self.base;
        }
        self.base + elapsed * self.speed
    }
}

struct Entry {
    source: String,
    layer: Arc<Layer>,
    pads: Arc<VideoPads>,
    clock: Clock,
}

pub struct Board {
    entries: Mutex<Vec<Entry>>,
    compositor: gst::Element,
    canvas: (i32, i32),
    probe: Mutex<Option<gst::PadProbeId>>,
}

impl Board {
    pub fn new(compositor: &gst::Element, canvas: (i32, i32)) -> Arc<Board> {
        Arc::new(Board { entries: Mutex::new(Vec::new()), compositor: compositor.clone(), canvas, probe: Mutex::new(None) })
    }

    /// Draw `layer` wherever the scene puts `source`. Installs the probe for
    /// the first one.
    pub fn attach(self: &Arc<Self>, source: &str, layer: Arc<Layer>, pads: Arc<VideoPads>) {
        let mut entries = self.entries.lock();
        entries.retain(|e| e.source != source);
        entries.push(Entry { source: source.to_string(), layer, pads, clock: Clock::default() });
        drop(entries);
        let mut probe = self.probe.lock();
        if probe.is_none() {
            *probe = super::draw::install(self);
        }
    }

    /// Stop drawing `source`. Takes the probe off with the last one.
    pub fn detach(&self, source: &str) {
        let mut entries = self.entries.lock();
        entries.retain(|e| e.source != source);
        let empty = entries.is_empty();
        drop(entries);
        if empty {
            if let (Some(id), Some(pad)) = (self.probe.lock().take(), self.compositor.static_pad("src")) {
                pad.remove_probe(id);
            }
        }
    }

    /// How many sources are on the board.
    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn compositor(&self) -> &gst::Element {
        &self.compositor
    }

    /// Everything to draw on the frame at `now`, bottom of the stack first.
    pub(super) fn jobs(&self, now: u64) -> Jobs {
        let mut jobs = Jobs::default();
        let mut entries = self.entries.lock();
        for e in entries.iter_mut() {
            if !e.layer.active() {
                continue;
            }
            let Some(pic) = e.layer.picture() else { continue };
            let motion = e.layer.motion();
            let travelled = match motion {
                Motion::Still => 0.0,
                Motion::Crawl { speed, .. } => e.clock.travelled(now, e.layer.epoch(), speed),
            };
            let mut biggest: Option<(u32, u32)> = None;
            let mut boxes: Vec<PadBox> = Vec::new();
            e.pads.each(|pad| {
                if pad.parent_element().as_ref() == Some(&self.compositor) {
                    boxes.extend(place::read(pad, self.canvas));
                }
            });
            for b in boxes {
                let (draws, size) = match motion {
                    Motion::Still => {
                        let (d, size) = place::still(&pic, &b);
                        (vec![d], size)
                    }
                    Motion::Crawl { .. } => place::crawl(&pic, &b, motion, travelled),
                };
                biggest = Some(biggest.map_or(size, |s| (s.0.max(size.0), s.1.max(size.1))));
                jobs.0.extend(draws.into_iter().map(|draw| Job { z: b.z, picture: pic.clone(), draw }));
            }
            if let Some(size) = biggest {
                e.layer.note_drawn(size);
            }
        }
        jobs.0.sort_by_key(|j| j.z);
        jobs
    }
}

/// Keep a transparent source's flattened picture off the compositor. Its
/// pad still exists and the scene still writes its place there; the board
/// reads that and draws the real picture, so the compositor must draw nothing.
///
/// On the head of the source's programme branch, so nothing below it in the
/// programme pipeline sees a buffer, and while `layer` is inactive (a clip
/// whose decoder turned out to have no alpha) everything passes as before.
pub fn hold_back(vtee: &gst::Element, layer: Arc<Layer>) {
    let Some(pad) = vtee.static_pad("sink") else { return };
    pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST, move |_, _| {
        if layer.active() {
            gst::PadProbeReturn::Drop
        } else {
            gst::PadProbeReturn::Ok
        }
    });
}

#[cfg(test)]
#[path = "board_tests.rs"]
mod tests;
