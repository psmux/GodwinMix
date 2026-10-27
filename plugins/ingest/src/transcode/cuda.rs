//! One CUDA context for the whole process on each card.
//!
//! Each converted stream is a pipeline of its own, and NVIDIA's elements
//! find the context they work in by asking their pipeline. Left alone, every
//! pipeline makes one, and fifty streams hold fifty contexts, each with its
//! own memory on the card. So the first context any element makes on a card
//! is kept here and handed to every element of that card after: the first
//! card's is set on the pipeline when it is made, which a bin hands to every
//! element added to it, and any element that asks is given its own card's.
//! A stream's elements are all on one card (the plan's `gpu`), so its
//! pictures stay in that card's memory from decode to encode.
//!
//! The context is made the first time an element asks for one, so a
//! machine that converts on the CPU never holds one, and it is made to
//! sleep while it waits for the GPU (`cuda_ctx.rs`). Where that cannot be
//! done, the first context an element makes for itself is kept instead.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use gstreamer as gst;
use gstreamer::prelude::*;

/// The context type GStreamer's CUDA elements share.
const CUDA: &str = "gst.cuda.context";

/// One context per card, by CUDA device.
static SHARED: Mutex<BTreeMap<u32, gst::Context>> = Mutex::new(BTreeMap::new());

fn held(device: u32) -> Option<gst::Context> {
    SHARED.lock().unwrap_or_else(|e| e.into_inner()).get(&device).cloned()
}

/// The card an NVIDIA element works on: its `cuda-device-id`, which is fixed
/// for a decoder or an encoder and set by the plan on a converter. One left
/// at -1 (any) is given the first card's, as before there were several.
fn device_of(el: &gst::Element) -> u32 {
    let Some(p) = el.find_property("cuda-device-id") else { return 0 };
    let v = el.property_value(p.name());
    v.get::<u32>().ok().or_else(|| v.get::<i32>().ok().and_then(|i| u32::try_from(i).ok())).unwrap_or(0)
}

/// Share the process's CUDA contexts with `pipeline`, and keep the first one
/// any of its elements makes on each card. A context is handed only to an
/// element of its own card, so no picture is copied from one card to another.
pub fn share(pipeline: &gst::Pipeline) {
    if let Some(c) = held(0) {
        // An element of another card refuses this one and asks for its own.
        pipeline.set_context(&c);
    }
    let Some(bus) = pipeline.bus() else { return };
    bus.set_sync_handler(|_, msg| {
        let el = msg.src().and_then(|s| s.downcast_ref::<gst::Element>().cloned());
        match (msg.view(), el) {
            (gst::MessageView::NeedContext(n), Some(el)) if n.context_type() == CUDA => {
                let device = device_of(&el);
                if let Some(c) = held(device).or_else(|| make(device)) {
                    el.set_context(&c);
                }
            }
            (gst::MessageView::HaveContext(h), Some(el)) if h.context().context_type() == CUDA => {
                SHARED.lock().unwrap_or_else(|e| e.into_inner()).entry(device_of(&el)).or_insert_with(|| h.context());
            }
            _ => {}
        }
        gst::BusSyncReply::Pass
    });
}

/// Make the shared context of a card, once; a failure is said once and
/// leaves it to GStreamer.
fn make(device: u32) -> Option<gst::Context> {
    static TRIED: Mutex<BTreeSet<u32>> = Mutex::new(BTreeSet::new());
    if TRIED.lock().unwrap_or_else(|e| e.into_inner()).insert(device) {
        match super::cuda_ctx::blocking(device) {
            Ok(c) => {
                SHARED.lock().unwrap_or_else(|e| e.into_inner()).entry(device).or_insert(c);
            }
            Err(why) => eprintln!("transcode: {why}; GStreamer's own CUDA context is used on card {device}, which spins while it waits"),
        }
    }
    held(device)
}
