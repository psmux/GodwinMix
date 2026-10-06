//! Where each decoded pad goes, and the thread that stops a player.

use super::Shared;
use crate::gstutil::make;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Video to the chain, anything else to a sink that throws it away.
pub fn route(bin: &gst::Pipeline, entry: &gst::Element, pad: &gst::Pad) {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    let video = caps.structure(0).is_some_and(|s| s.name().starts_with("video/"));
    if video {
        if let Some(sink) = entry.static_pad("sink").filter(|p| !p.is_linked()) {
            let _ = pad.link(&sink);
        }
        return;
    }
    let Ok(drop) = make("fakesink", &format!("fx-drop-{}", pad.name())) else { return };
    drop.set_property("sync", false);
    drop.set_property("async", false);
    if bin.add(&drop).is_ok() {
        let _ = drop.sync_state_with_parent();
        if let Some(sink) = drop.static_pad("sink") {
            let _ = pad.link(&sink);
        }
    }
}

/// Stop the pipeline when the pass is done with it, when it fails, or when
/// it has sat at its end for two seconds with nobody drawing.
pub fn watch(pipeline: gst::Pipeline, shared: Arc<Shared>) {
    let started = std::thread::Builder::new().name("gmx-fx-player".into()).spawn(move || {
        let Some(bus) = pipeline.bus() else { return };
        let mut ended: Option<Instant> = None;
        while !shared.done.load(Ordering::Acquire) {
            let msg = bus.timed_pop_filtered(gst::ClockTime::from_mseconds(50), &[gst::MessageType::Eos, gst::MessageType::Error]);
            match msg.as_ref().map(|m| m.view()) {
                Some(gst::MessageView::Error(e)) => {
                    warn!(error = %e.error(), "an effect clip would not play");
                    shared.failed.store(true, Ordering::Release);
                    break;
                }
                Some(gst::MessageView::Eos(_)) => ended = Some(Instant::now()),
                _ => {}
            }
            if ended.is_some_and(|t| t.elapsed() > Duration::from_secs(2)) {
                break;
            }
        }
        shared.done.store(true, Ordering::Release);
        let _ = pipeline.set_state(gst::State::Null);
        debug!("an effect clip stopped");
        if let Some(f) = shared.on_end.lock().take() {
            f();
        }
    });
    if let Err(e) = started {
        warn!(error = %e, "could not start the thread that watches an effect clip");
    }
}
