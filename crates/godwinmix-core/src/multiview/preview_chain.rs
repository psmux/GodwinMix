//! An item's filters on its slot in the preview.
//!
//! The programme draws an item's chroma key, cutout, look or colour on the
//! item's own slot, between the slot's flip and its compositor pad
//! (`mixer::slots`). The preview drew the same item from the source's
//! thumbnail with no filters at all, so a keyed presenter stood in front of
//! her green screen in Preview and in front of the studio set on air. This is
//! the same chain on the preview's slot:
//!
//! ```text
//!   tile tee > q > crop > flip > [filter] > [filter] > preview pad
//! ```
//!
//! It is built with the slot, before the slot is started, so nothing is
//! inserted under a pad block and the mixer thread never waits on a frame.
//! A change of settings on the same filters is applied in place, which is a
//! slider dragged in the composer; a change of which filters is a new slot,
//! because a preview slot is cheap to make and the old one goes at the end of
//! the same apply.
//!
//! A filter whose result has alpha hands its picture to the preview's board
//! (`transparent`), as on air it hands it to the programme's. A cutout says
//! whose picture it cuts (`Filter::share_as`), so while the same camera is
//! cut out on air the preview reads that model's masks instead of running a
//! second one.
//!
//! What it costs is the filters' own work on a thumbnail sized frame
//! (`input::THUMB_WIDTH`), and only while a client is watching the preview
//! and the armed scene has an item with a filter.

use super::transparent::Transparent;
use crate::caps::CanvasCaps;
use crate::mixer::slots::ItemFilter;
use crate::plugin::filter::{self, BoardHook, FilterSlot, FilterSpec, Insertion};
use crate::plugin::{Configure, FilterSide};
use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::warn;

#[derive(Default)]
pub struct Chain {
    filters: Vec<FilterSlot>,
    /// What was asked for, to compare the next apply against.
    shape: Vec<ItemFilter>,
    /// The board names of the filters drawn by the board, beside their hooks.
    keyed: Vec<(String, BoardHook)>,
}

/// Where a chain goes, and whose picture it filters.
pub struct At<'a> {
    pub pipeline: &'a gst::Pipeline,
    pub flip: &'a gst::Element,
    pub pad: &'a gst::Pad,
    pub source: &'a str,
    /// The slot's own number, which keeps its board names apart from every
    /// other slot's.
    pub serial: u64,
    /// The programme's canvas. No filter reads it (they take the caps that
    /// flow, here a thumbnail's), but it is what a filter is built against.
    pub canvas: &'a CanvasCaps,
}

impl Chain {
    /// Put `want` between the flip and the pad, which must already be linked
    /// and not yet started. A filter that will not build takes the whole
    /// chain off with it, as on the programme: the item is drawn clean rather
    /// than half filtered.
    pub fn build(at: &At<'_>, want: &[ItemFilter], board: &Transparent) -> Chain {
        let mut chain = Chain { shape: want.to_vec(), ..Chain::default() };
        for (n, f) in want.iter().enumerate() {
            if let Err(e) = chain.push(at, n, f, board) {
                warn!(source = at.source, filter = %f.type_id, ?e, "a filter could not be drawn in the preview; the item is shown without its filters");
                chain.teardown(at.pipeline, board);
                chain.shape = want.to_vec();
                return chain;
            }
        }
        chain
    }

    fn push(&mut self, at: &At<'_>, n: usize, f: &ItemFilter, board: &Transparent) -> Result<()> {
        let mut made = filter::make(&f.type_id)?;
        made.share_as(at.source, false);
        let key = format!("preview {} filter {n}", at.serial);
        // On a programme that keeps alpha itself the key flattens over black
        // on air, so it does here too: the preview shows what air will show.
        let hook = made.board().filter(|_| !crate::mixer::programme_keeps_alpha());
        if let Some(hook) = &hook {
            board.key(&key, hook, at.pad);
        }
        let upstream = self.filters.last().map(|f| f.bin.clone()).unwrap_or_else(|| at.flip.clone());
        let spec = FilterSpec {
            id: format!("pv-{}-filter-{n}", at.serial),
            type_id: f.type_id.clone(),
            side: FilterSide::SceneItem,
            params: f.params.clone(),
        };
        let placed = filter::insert(Insertion::before_pad(at.pipeline, &upstream, at.pad), spec, made, at.canvas, false);
        match placed {
            Ok(slot) => {
                self.filters.push(slot);
                if let Some(hook) = hook {
                    self.keyed.push((key, hook));
                }
                Ok(())
            }
            Err(e) => {
                if let Some(hook) = &hook {
                    board.unkey(&key, hook);
                }
                Err(e)
            }
        }
    }

    /// The bins, downstream last, for the slot to start and stop with its own.
    pub fn elements(&self) -> Vec<gst::Element> {
        self.filters.iter().map(|f| f.bin.clone()).collect()
    }

    /// True when this chain is exactly what `want` asks for.
    pub fn is(&self, want: &[ItemFilter]) -> bool {
        self.shape == want
    }

    /// True when this chain has the same filters of the same kinds, in order,
    /// so `configure` can take the rest.
    pub fn same_kinds(&self, want: &[ItemFilter]) -> bool {
        self.shape.len() == want.len() && self.shape.iter().zip(want).all(|(a, b)| a.type_id == b.type_id)
    }

    /// Bring the settings in line with `want`, in place. False when a filter
    /// refused the change or the chain is not the one asked for, and the
    /// caller makes a new slot instead.
    pub fn configure(&mut self, want: &[ItemFilter]) -> bool {
        if self.shape == want {
            return true;
        }
        if !self.same_kinds(want) || self.filters.len() != want.len() {
            return false;
        }
        for (slot, f) in self.filters.iter_mut().zip(want) {
            if slot.spec.params == f.params {
                continue;
            }
            if !matches!(slot.configure(&f.params), Ok(Configure::Applied)) {
                return false;
            }
            // `configure` merges, as `filter.set` does. A key the item no
            // longer has goes back to the filter's default with a new slot,
            // so the merge must not hide it.
            if slot.spec.params != f.params {
                return false;
            }
        }
        self.shape = want.to_vec();
        true
    }

    /// Off the board and out of the pipeline. Called by the slot after its
    /// pads are given back, so nothing is pushing into these any more.
    pub fn teardown(&mut self, pipeline: &gst::Pipeline, board: &Transparent) {
        for (key, hook) in self.keyed.drain(..) {
            board.unkey(&key, &hook);
        }
        for slot in self.filters.drain(..) {
            slot.bin.set_locked_state(true);
            let _ = slot.bin.set_state(gst::State::Null);
            let _ = pipeline.remove(&slot.bin);
        }
        self.shape.clear();
    }
}
