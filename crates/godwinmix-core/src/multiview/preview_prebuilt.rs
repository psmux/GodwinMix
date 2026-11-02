//! An item's filters for the preview, built off the mixer loop.
//!
//! The slow half of a preview chain: each filter made, told whose picture it
//! works on, and built into a bin that is in no pipeline yet. A look reads
//! its file here and a cutout starts its worker here. `mixer::preview_draw`
//! runs it on a blocking worker and hands the result to the loop, which
//! places the bins (`preview_chain::Chain::build`).

use crate::caps::CanvasCaps;
use crate::mixer::slots::ItemFilter;
use crate::plugin::filter::{self, Built, FilterSpec};
use crate::plugin::FilterSide;
use anyhow::Result;

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
