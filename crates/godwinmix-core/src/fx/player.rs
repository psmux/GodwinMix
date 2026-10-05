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
        software_only(&decode);
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

    /// Run `f` on the clip's streaming thread when its first frame is ready.
    /// `f` must only hand the news on, never wait.
    pub fn on_first_frame(&self, f: impl Fn() + Send + Sync + 'static) {
        self.sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_preroll(move |_| {
                    f();
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
    }

    /// Stop now, whatever has been drawn.
    pub fn stop(&self) {
        self.shared.done.store(true, Ordering::Release);
    }
}

/// Decode on the CPU. A clip here is small and short, and a hardware decoder
/// is a plugin to load in the middle of a show: on Windows the Quick Sync
/// one has been seen to corrupt the heap of the process that loads it.
/// `force-sw-decoders` is in decodebin from GStreamer 1.22; an older one
/// keeps its own choice.
pub fn software_only(decode: &gst::Element) {
    crate::probe::set_bool(decode, "force-sw-decoders", true);
}

#[path = "player_watch.rs"]
mod watch;
use watch::{route, watch};
