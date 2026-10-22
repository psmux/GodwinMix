//! One CUDA context for the whole process.
//!
//! Each converted stream is a pipeline of its own, and NVIDIA's elements
//! find the context they work in by asking their pipeline. Left alone, every
//! pipeline makes one, and fifty streams hold fifty contexts, each with its
//! own memory on the card. So the first context any element makes is kept
//! here and handed to every pipeline after: set on the pipeline when it is
//! made, which a bin hands to every element added to it, and given to any
//! element that asks before there is one to give.

use std::sync::Mutex;

use gstreamer as gst;
use gstreamer::prelude::*;

/// The context type GStreamer's CUDA elements share.
const CUDA: &str = "gst.cuda.context";

static SHARED: Mutex<Option<gst::Context>> = Mutex::new(None);

fn held() -> Option<gst::Context> {
    SHARED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Share the process's CUDA context with `pipeline`, and keep the first one
/// any of its elements makes.
pub fn share(pipeline: &gst::Pipeline) {
    if let Some(c) = held() {
        pipeline.set_context(&c);
    }
    let Some(bus) = pipeline.bus() else { return };
    bus.set_sync_handler(|_, msg| {
        match msg.view() {
            gst::MessageView::NeedContext(n) if n.context_type() == CUDA => {
                let asker = msg.src().and_then(|s| s.downcast_ref::<gst::Element>().cloned());
                if let (Some(c), Some(el)) = (held(), asker) {
                    el.set_context(&c);
                }
            }
            gst::MessageView::HaveContext(h) if h.context().context_type() == CUDA => {
                let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
                if shared.is_none() {
                    *shared = Some(h.context());
                }
            }
            _ => {}
        }
        gst::BusSyncReply::Pass
    });
}

/// Whether a context has been made yet, for a test.
#[cfg(test)]
pub fn made() -> bool {
    held().is_some()
}
