//! One CUDA context for the whole process.
//!
//! Each converted stream is a pipeline of its own, and NVIDIA's elements
//! find the context they work in by asking their pipeline. Left alone, every
//! pipeline makes one, and fifty streams hold fifty contexts, each with its
//! own memory on the card. So the first context any element makes is kept
//! here and handed to every pipeline after: set on the pipeline when it is
//! made, which a bin hands to every element added to it, and given to any
//! element that asks before there is one to give.
//!
//! The context is made the first time an element asks for one, so a
//! machine that converts on the CPU never holds one, and it is made to
//! sleep while it waits for the GPU (`cuda_ctx.rs`). Where that cannot be
//! done, the first context an element makes for itself is kept instead.

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
                if let (Some(c), Some(el)) = (held().or_else(make), asker) {
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

/// Make the shared context, once; a failure is said once and leaves it to
/// GStreamer.
fn make() -> Option<gst::Context> {
    static TRIED: std::sync::Once = std::sync::Once::new();
    TRIED.call_once(|| match super::cuda_ctx::blocking() {
        Ok(c) => {
            let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
            shared.get_or_insert(c);
        }
        Err(why) => eprintln!("transcode: {why}; GStreamer's own CUDA context is used, which spins while it waits"),
    });
    held()
}
