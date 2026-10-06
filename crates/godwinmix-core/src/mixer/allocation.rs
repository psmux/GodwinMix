//! The compositor's allocation query, answered from memory when nothing about
//! it has changed.
//!
//! A slot whose crop changes size every frame (a wipe, a box, an item that
//! wipes in) hands the compositor new input caps on every frame.
//! `videoaggregator` marks its src pad for renegotiation each time it takes
//! such a buffer, and renegotiating sends an ALLOCATION query downstream from
//! the compositor's own thread. The query is serialized, so every `queue` on
//! the way holds it until it has pushed out what it already has, which is the
//! encoder's backlog. Measured on a debug core at 320x180, a one second wipe
//! sent 30 of them: the longest took 67 ms on a quiet machine and 113 ms
//! under load, and while it waits the compositor makes no frame and takes no
//! buffer from any slot. On a loaded machine that was the incoming slot
//! passing no frame at all for half a second and the programme catching up
//! in bursts.
//!
//! The output caps never changed (`vmix-caps` fixes them), so downstream's
//! answer does not change either. The first query for a set of caps goes
//! down and its answer is kept; a repeat for the same caps is answered here
//! with it. Only an answer with no pool object in it is kept: a pool is
//! handed to one negotiation and configured there, so an answer that offers
//! one (a GPU path) goes down every time as it always did. A RECONFIGURE from downstream (an output attached or taken
//! away) forgets the answer, so the next query asks again.

use glib::translate::IntoGlib;
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

/// What downstream said, for one set of caps.
#[derive(Debug, Clone)]
struct Answer {
    caps: gst::Caps,
    need_pool: bool,
    /// Size, min and max of each pool entry, none of which named a pool.
    sizes: Vec<(u32, u32, u32)>,
    /// The allocators offered, which are shared objects with no state of
    /// their own to hand out twice, unlike a pool.
    allocators: Vec<(Option<gst::Allocator>, gst::AllocationParams)>,
    metas: Vec<(glib::Type, Option<gst::Structure>)>,
}

type Memory = Arc<Mutex<Option<Answer>>>;

/// Keep the compositor's allocation answer and replay it. See the module.
pub fn remember_answers(vmix: &gst::Element) {
    let Some(src) = vmix.static_pad("src") else { return };
    let memory: Memory = Arc::default();
    let (ask, learn, forget) = (memory.clone(), memory.clone(), memory);
    src.add_probe(gst::PadProbeType::QUERY_DOWNSTREAM | gst::PadProbeType::PUSH, move |_, info| {
        let Some(gst::PadProbeData::Query(query)) = &mut info.data else { return gst::PadProbeReturn::Ok };
        match replay(&ask.lock(), query) {
            true => gst::PadProbeReturn::Handled,
            false => gst::PadProbeReturn::Ok,
        }
    });
    src.add_probe(gst::PadProbeType::QUERY_DOWNSTREAM | gst::PadProbeType::PULL, move |_, info| {
        if let Some(gst::PadProbeData::Query(query)) = &info.data {
            if let gst::QueryView::Allocation(a) = query.view() {
                *learn.lock() = kept(a);
            }
        }
        gst::PadProbeReturn::Ok
    });
    src.add_probe(gst::PadProbeType::EVENT_UPSTREAM, move |_, info| {
        if info.event().is_some_and(|e| e.type_() == gst::EventType::Reconfigure) {
            *forget.lock() = None;
        }
        gst::PadProbeReturn::Ok
    });
}

/// The answer downstream gave, if it is one that can be given again.
fn kept(a: &gst::query::Allocation) -> Option<Answer> {
    let (caps, need_pool) = a.get();
    let caps = caps?.to_owned();
    let allocators = a.allocation_params().collect();
    let mut sizes = Vec::new();
    for (pool, size, min, max) in a.allocation_pools() {
        if pool.is_some() {
            return None;
        }
        sizes.push((size, min, max));
    }
    let metas = a.allocation_metas().map(|(api, params)| (api, params.map(|p| p.to_owned()))).collect();
    Some(Answer { caps, need_pool, sizes, allocators, metas })
}

/// Answer an allocation query from memory. False when it has to go down.
fn replay(memory: &Option<Answer>, query: &mut gst::QueryRef) -> bool {
    let Some(answer) = memory else { return false };
    let gst::QueryViewMut::Allocation(a) = query.view_mut() else { return false };
    let (caps, need_pool) = a.get();
    if need_pool != answer.need_pool || caps.is_none_or(|c| c != answer.caps.as_ref()) {
        return false;
    }
    for (size, min, max) in &answer.sizes {
        a.add_allocation_pool(None::<&gst::BufferPool>, *size, *min, *max);
    }
    for (allocator, params) in &answer.allocators {
        a.add_allocation_param(allocator.as_ref(), *params);
    }
    for (api, params) in &answer.metas {
        // The safe binding is generic over a Rust meta type; the API here is
        // only known as the GType downstream named.
        unsafe {
            gst::ffi::gst_query_add_allocation_meta(
                a.as_mut_ptr(),
                api.into_glib(),
                params.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
            );
        }
    }
    true
}

#[cfg(test)]
mod tests;
