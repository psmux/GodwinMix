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
//! The filters are built off the mixer loop (`prebuild`, called from
//! `mixer::preview_draw`), and placed with the slot, before the slot is
//! started, so nothing is inserted under a pad block, the mixer thread never
//! waits on a frame, and a Take never waits behind a LUT file read. A change of settings on the same filters is applied in place, which is a
//! slider dragged in the composer; a change of which filters is a new slot,
//! because a preview slot is cheap to make and the old one goes at the end of
//! the same apply.
//!
//! A filter whose result has alpha hands its picture to the preview's board
//! (`transparent`), as on air it hands it to the programme's. A cutout says
//! whose picture it cuts (`Filter::share_as`): the source with its crop and
//! turn. While the same picture is cut out on air the preview reads that
//! model's masks instead of running a second one; the same camera cropped
//! differently is another picture and another model.
//!
//! What it costs is the filters' own work on a thumbnail sized frame
//! (`input::THUMB_WIDTH`), and only while a client is watching the preview
//! and the armed scene has an item with a filter.

use super::transparent::Transparent;
use crate::caps::CanvasCaps;
use crate::mixer::slots::ItemFilter;
use crate::plugin::filter::{self, BoardHook, Built, FilterSlot, FilterSpec, Insertion};
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
    /// The picture the filters were last told they work on
    /// (`filter::share_key`).
    shared_as: String,
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
    /// The cell's picture, `filter::share_key` of its source, crop and turn.
    pub key: String,
}

/// Build a cell's filters without placing them: the slow half, done off the
/// mixer loop (`mixer::preview_draw`) so a Take never waits behind a LUT file
/// read or a cutout starting. `canvas` is the programme's: no filter reads it
/// (they take the caps that flow, here a thumbnail's), but it is what a
/// filter is built against.
pub fn prebuild(key: &str, want: &[ItemFilter], canvas: &CanvasCaps) -> Result<Vec<Built>> {
    want.iter()
        .enumerate()
        .map(|(n, f)| {
            let mut made = filter::make(&f.type_id)?;
            made.share_as(key, false);
            let spec = FilterSpec {
                id: format!("pv-filter-{n}"),
                type_id: f.type_id.clone(),
                side: FilterSide::SceneItem,
                params: f.params.clone(),
            };
            Built::new(spec, made, canvas)
        })
        .collect()
}

impl Chain {
    /// Put `want`, built already by `prebuild`, between the flip and the pad,
    /// which must already be linked and not yet started. A filter that will
    /// not build or link takes the whole chain off with it, as on the
    /// programme: the item is drawn clean rather than half filtered.
    pub fn build(at: &At<'_>, want: &[ItemFilter], built: Result<Vec<Built>>, board: &Transparent) -> Chain {
        let mut chain = Chain { shape: want.to_vec(), shared_as: at.key.clone(), ..Chain::default() };
        let placed = built.and_then(|built| {
            built.into_iter().enumerate().try_for_each(|(n, b)| chain.push(at, n, b, board))
        });
        if let Err(e) = placed {
            warn!(source = at.source, ?e, "a filter could not be drawn in the preview; the item is shown without its filters");
            chain.teardown(at.pipeline, board);
            relink_clean(at);
            chain.shape = want.to_vec();
        }
        chain
    }

    fn push(&mut self, at: &At<'_>, n: usize, mut built: Built, board: &Transparent) -> Result<()> {
        let key = format!("preview {} filter {n}", at.serial);
        // On a programme that keeps alpha itself the key flattens over black
        // on air, so it does here too: the preview shows what air will show.
        let hook = built.filter.board().filter(|_| !crate::mixer::programme_keeps_alpha());
        if let Some(hook) = &hook {
            board.key(&key, hook, at.pad);
        }
        let upstream = self.filters.last().map(|f| f.bin.clone()).unwrap_or_else(|| at.flip.clone());
        built.spec.id = format!("pv-{}-filter-{n}", at.serial);
        match filter::place(Insertion::before_pad(at.pipeline, &upstream, at.pad), built, false) {
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

    /// Tell the filters their picture changed: the cell's crop or turn did,
    /// on a slot that is kept.
    pub fn reshare(&mut self, key: &str) {
        if self.shared_as == key {
            return;
        }
        for f in &mut self.filters {
            f.share_as(key, false);
        }
        self.shared_as = key.to_string();
    }

    /// The hooks of the filters drawn by the board.
    pub fn hooks(&self) -> impl Iterator<Item = crate::plugin::filter::BoardHook> + '_ {
        self.keyed.iter().map(|(_, h)| h.clone())
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

/// The flip straight to the pad, after a chain that would not build has come
/// off. The filters that did go in were taken out with their links, so the
/// flip is linked to nothing and the item would vanish from Preview.
fn relink_clean(at: &At<'_>) {
    let Some(src) = at.flip.static_pad("src") else { return };
    if let Some(peer) = src.peer() {
        if &peer == at.pad {
            return;
        }
        let _ = src.unlink(&peer);
    }
    if let Some(peer) = at.pad.peer() {
        let _ = peer.unlink(at.pad);
    }
    if let Err(e) = src.link(at.pad) {
        warn!(source = at.source, ?e, "could not link the item straight to the preview after its filters failed");
    }
}

/// Chains built off the loop for the cells that will need new slots, each
/// beside the picture and the filters it was built for. What an apply does
/// not use is dropped with it: a bin never added to a pipeline needs nothing
/// more.
#[derive(Default)]
pub struct Prebuilt {
    ready: Vec<(String, Vec<ItemFilter>, Result<Vec<Built>>)>,
}

impl Prebuilt {
    /// Build every chain in `wanted`. Slow, and meant for a worker thread.
    pub fn build(wanted: Vec<(String, Vec<ItemFilter>)>, canvas: &CanvasCaps) -> Prebuilt {
        let ready = wanted
            .into_iter()
            .map(|(key, want)| {
                let built = prebuild(&key, &want, canvas);
                (key, want, built)
            })
            .collect();
        Prebuilt { ready }
    }

    /// The chain built for this picture and these filters, if there is one;
    /// built here and now if not, which is a slot `pick` could not keep after
    /// all and costs the loop what it always did.
    pub fn take(&mut self, key: &str, want: &[ItemFilter], canvas: &CanvasCaps) -> Result<Vec<Built>> {
        match self.ready.iter().position(|(k, w, _)| k == key && w == want) {
            Some(i) => self.ready.swap_remove(i).2,
            None => prebuild(key, want, canvas),
        }
    }

    /// True when a chain for this picture and these filters is here.
    pub fn has(&self, key: &str, want: &[ItemFilter]) -> bool {
        self.ready.iter().any(|(k, w, _)| k == key && w == want)
    }

    pub fn extend(&mut self, more: Prebuilt) {
        self.ready.extend(more.ready);
    }

    pub fn len(&self) -> usize {
        self.ready.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ready.is_empty()
    }
}

#[cfg(test)]
#[path = "preview_chain_tests.rs"]
mod tests;
