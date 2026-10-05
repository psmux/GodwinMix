//! A clip decoded on a pipeline of its own, for a pass to draw from.
//!
//! ```text
//!   uridecodebin =| queue -> videoconvert -> videoscale -> AYUV at the canvas size -> appsink
//!                \-> fakesink (sound, for now)
//! ```
//!
//! Its own pipeline rather than a source in the programme: an effect has no
//! slot, no tile and no scene item, and a clip that will not decode costs
//! that clip and nothing else. The sink does not sync to a clock. It holds
//! three frames and the decoder waits behind them, so the clip is decoded a
//! few frames ahead and the pass takes each frame when the programme's
//! running time reaches it. A clip that decodes late is drawn late, frame by
//! frame; the programme is never held for it.
//!
//! A thread per player watches its bus and stops it, off every streaming
//! thread, once the pass has drawn the last frame or the owner says stop.

use crate::gstutil::make;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use parking_lot::Mutex;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

type OnEnd = Box<dyn FnOnce() + Send>;

/// What the player, its watcher and the pass share.
#[derive(Default)]
pub struct Shared {
    /// Set by the pass when it has drawn the last frame, or by `stop`.
    pub done: AtomicBool,
    /// The clip would not play. The pass draws nothing more.
    pub failed: AtomicBool,
    on_end: Mutex<Option<OnEnd>>,
}

pub struct Player {
    pub sink: gst_app::AppSink,
    pub shared: Arc<Shared>,
}

impl Player {
    /// Start decoding `path` into AYUV frames `size` big.
    pub fn start(path: &Path, size: (i32, i32)) -> Result<Player> {
        let pipeline = gst::Pipeline::with_name("fx-player");
        let uri = gst::glib::filename_to_uri(path, None).context("the clip's path is not a file")?;
        let decode = make("uridecodebin", "fx-decode")?;
        decode.set_property("uri", uri.as_str());
        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "AYUV")
            .field("width", size.0)
            .field("height", size.1)
            .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
            .field("colorimetry", crate::caps::COLORIMETRY)
            .build();
        let chain = [make("queue", "fx-q")?, make("videoconvert", "fx-convert")?, make("videoscale", "fx-scale")?, crate::gstutil::capsfilter("fx-caps", &caps)?];
        let sink = gst_app::AppSink::builder().name("fx-sink").sync(false).max_buffers(3).drop(false).build();
        pipeline.add(&decode)?;
        pipeline.add_many(&chain)?;
        pipeline.add(&sink)?;
        gst::Element::link_many(&chain)?;
        chain[3].link(&sink)?;
        let entry = chain[0].clone();
        let bin = pipeline.clone();
        decode.connect_pad_added(move |_, pad| route(&bin, &entry, pad));
        pipeline.set_state(gst::State::Playing).context("starting the clip")?;
        let shared = Arc::new(Shared::default());
        watch(pipeline, shared.clone());
        Ok(Player { sink, shared })
    }

    /// Run `f` once the player has stopped, on the watcher's thread.
    pub fn on_end(&self, f: impl FnOnce() + Send + 'static) {
        *self.shared.on_end.lock() = Some(Box::new(f));
    }

    /// Stop now, whatever has been drawn.
    pub fn stop(&self) {
        self.shared.done.store(true, Ordering::Release);
    }
}

/// Video to the chain, anything else to a sink that throws it away.
fn route(bin: &gst::Pipeline, entry: &gst::Element, pad: &gst::Pad) {
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
fn watch(pipeline: gst::Pipeline, shared: Arc<Shared>) {
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
