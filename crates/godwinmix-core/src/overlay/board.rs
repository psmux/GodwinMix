//! The board: every transparent source on this programme, and the one pad
//! probe that draws them.
//!
//! The probe sits on the compositor's src pad and is there only while at
//! least one transparent source exists. With none it is not installed at all,
//! so a show with no text, ticker or transparent picture runs exactly the
//! graph it ran before this module.

use super::clock::Clock;
use super::layer::Layer;
use super::picture::Motion;
use super::place::{self, PadBox};
use super::draw::{Job, Jobs};
use crate::plugin::branch::VideoPads;
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

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
    /// Effects, stinger clips and frame transitions over the whole frame.
    pub(super) passes: Mutex<super::pass::Passes>,
    /// Whether the size drawn at goes back to the kind, which renders at it.
    /// The programme's board does; the preview's does not, or the two would
    /// ask for different sizes in turn and the text would never stop being
    /// rendered again.
    measures: bool,
}

impl Board {
    pub fn new(compositor: &gst::Element, canvas: (i32, i32)) -> Arc<Board> {
        Arc::new(Board { entries: Mutex::new(Vec::new()), compositor: compositor.clone(), canvas, probe: Mutex::new(None), passes: Mutex::default(), measures: true })
    }

    /// A board that draws what another one's layers hold without asking them
    /// to render at its size: the scene preview's, at thumbnail size.
    pub fn watching(compositor: &gst::Element, canvas: (i32, i32)) -> Arc<Board> {
        Arc::new(Board { entries: Mutex::new(Vec::new()), compositor: compositor.clone(), canvas, probe: Mutex::new(None), passes: Mutex::default(), measures: false })
    }

    /// The layer `source` draws from on this board, if it is a transparent one.
    pub fn layer_of(&self, source: &str) -> Option<Arc<Layer>> {
        self.entries.lock().iter().find(|e| e.source == source).map(|e| e.layer.clone())
    }

    /// Draw `layer` wherever the scene puts `source`. Installs the probe for
    /// the first one.
    pub fn attach(self: &Arc<Self>, source: &str, layer: Arc<Layer>, pads: Arc<VideoPads>) {
        let mut entries = self.entries.lock();
        entries.retain(|e| e.source != source);
        entries.push(Entry { source: source.to_string(), layer, pads, clock: Clock::default() });
        drop(entries);
        self.ensure_probe();
    }

    /// Stop drawing `source`. Takes the probe off with the last one.
    pub fn detach(&self, source: &str) {
        let mut entries = self.entries.lock();
        entries.retain(|e| e.source != source);
        drop(entries);
        self.drop_probe_if_idle();
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
                    Motion::Crawl { .. } => {
                        if let Some(bar) = e.layer.backdrop() {
                            let filled = PadBox { fit: place::Fit::Fill, ..b };
                            let (draw, _) = place::still(&bar, &filled);
                            jobs.0.push(Job { z: b.z, picture: bar, draw });
                        }
                        place::crawl(&pic, &b, motion, travelled)
                    }
                };
                biggest = Some(biggest.map_or(size, |s| (s.0.max(size.0), s.1.max(size.1))));
                jobs.0.extend(draws.into_iter().map(|draw| Job { z: b.z, picture: pic.clone(), draw }));
            }
            if let (true, Some(size)) = (self.measures, biggest) {
                e.layer.note_drawn(size);
            }
        }
        // Stable, so a ticker's bar stays under its words.
        jobs.0.sort_by_key(|j| j.z);
        jobs
    }
}

pub use super::hold::{hold_back, hold_back_at};

#[path = "board_passes.rs"]
mod passes;
