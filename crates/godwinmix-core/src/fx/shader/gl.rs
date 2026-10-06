//! A shader on the GPU, through GStreamer GL, on a pipeline of its own.
//!
//! ```text
//!   appsrc (old above new, one I420 frame twice the height)
//!     -> glupload -> glcolorconvert -> glshader -> glcolorconvert -> gldownload -> appsink
//! ```
//!
//! `glshader` reads one texture, so both scenes go up in one: the old
//! picture in the top half and the new in the bottom, and the shader's
//! `getFromColor` and `getToColor` look in each half. The answer is drawn
//! into the top half and read back from there.
//!
//! The programme's thread never waits for the GPU. Each frame it hands the
//! two pictures over and draws the newest answer that has come back, which
//! is the previous frame's: during a shader transition the new scene is one
//! frame behind, and nobody watching a ripple can tell. Before the first
//! answer it draws the old picture, which is what progress near 0 shows
//! anyway.

use super::super::frame::Pic;
use crate::overlay::blend::Planes;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Gl {
    pipeline: gst::Pipeline,
    src: gst_app::AppSrc,
    sink: gst_app::AppSink,
    shader: gst::Element,
    stacked: gst_video::VideoInfo,
    latest: Mutex<Option<gst::Buffer>>,
    failed: Arc<AtomicBool>,
    ratio: f32,
    /// Stopped already, by `close`, so dropping it has nothing to do.
    closed: AtomicBool,
}

impl Gl {
    /// Build the pipeline and start it. Called off every streaming thread,
    /// because building it loads the GL plugin.
    pub fn start(fragment: &str, size: (i32, i32)) -> Result<Gl> {
        let stacked = gst_video::VideoInfo::builder(gst_video::VideoFormat::I420, size.0 as u32, size.1 as u32 * 2)
            .fps(gst::Fraction::new(30, 1))
            .build()
            .context("the stacked frame's layout")?;
        let desc = "appsrc name=src format=time is-live=false do-timestamp=false max-buffers=2 block=false \
                    ! glupload ! glcolorconvert ! glshader name=shader ! glcolorconvert ! gldownload \
                    ! video/x-raw,format=I420 ! appsink name=sink sync=false max-buffers=2 drop=true";
        let pipeline = gst::parse::launch(desc)?.downcast::<gst::Pipeline>().map_err(|_| anyhow::anyhow!("not a pipeline"))?;
        let src = pipeline.by_name("src").context("no appsrc")?.downcast::<gst_app::AppSrc>().map_err(|_| anyhow::anyhow!("not an appsrc"))?;
        let sink = pipeline.by_name("sink").context("no appsink")?.downcast::<gst_app::AppSink>().map_err(|_| anyhow::anyhow!("not an appsink"))?;
        let shader = pipeline.by_name("shader").context("no glshader")?;
        src.set_caps(Some(&stacked.to_caps()?));
        shader.set_property("fragment", fragment);
        let ratio = size.0 as f32 / size.1.max(1) as f32;
        let gl = Gl { pipeline, src, sink, shader, stacked, latest: Mutex::new(None), failed: Arc::default(), ratio, closed: AtomicBool::new(false) };
        gl.uniforms(0.0);
        // Not waited for: the GL context is made when the first frame
        // arrives, and a shader that will not compile says so on the bus,
        // which `mix` reads every frame.
        gl.pipeline.set_state(gst::State::Playing).context("GStreamer GL would not start")?;
        if let Some(e) = gl.error() {
            anyhow::bail!("GStreamer GL would not run the shader: {e}");
        }
        Ok(gl)
    }

    fn uniforms(&self, t: f64) {
        let s = gst::Structure::builder("uniforms").field("progress", t as f32).field("ratio", self.ratio).build();
        self.shader.set_property("uniforms", s);
    }

    /// The first error on the pipeline's bus, if there is one.
    pub fn error(&self) -> Option<String> {
        let bus = self.pipeline.bus()?;
        let msg = bus.pop_filtered(&[gst::MessageType::Error])?;
        let gst::MessageView::Error(e) = msg.view() else { return None };
        self.failed.store(true, Ordering::Release);
        Some(format!("{} ({})", e.error(), e.debug().unwrap_or_default()))
    }

    /// Draw the newest answer onto `f`, then send this frame's pictures.
    pub fn mix(&self, old: &Pic<'_>, f: &mut Planes<'_>, t: f64) {
        if self.failed.load(Ordering::Acquire) || self.error().is_some() {
            return super::super::matte::dissolve(old, f, t);
        }
        let stacked = frames::stack(&self.stacked, old, f);
        while let Some(s) = self.sink.try_pull_sample(gst::ClockTime::ZERO) {
            *self.latest.lock() = s.buffer_owned();
        }
        if let Some(answer) = self.latest.lock().clone() {
            frames::draw(&self.stacked, &answer, f);
        } else {
            let pic = Pic { y: old.y, u: old.u, v: old.v, strides: old.strides };
            super::super::matte::dissolve(&pic, f, 0.0);
        }
        if let Some(buffer) = stacked {
            self.uniforms(t);
            let _ = self.src.push_buffer(buffer);
        }
    }

    /// Whether an answer has come back yet.
    pub fn answered(&self) -> bool {
        self.latest.lock().is_some()
    }

    /// Draw the newest answer onto `f` and send nothing, for a caller that
    /// waited for the answer to the one frame it sent.
    pub fn draw_latest(&self, f: &mut Planes<'_>) {
        if let Some(answer) = self.latest.lock().clone() {
            frames::draw(&self.stacked, &answer, f);
        }
    }

    /// Drop every answer so far, for a caller about to wait for a new one.
    pub fn forget(&self) {
        while self.sink.try_pull_sample(gst::ClockTime::ZERO).is_some() {}
        *self.latest.lock() = None;
    }

    /// Take in what has come back, without drawing it.
    pub fn answered_now(&self) -> bool {
        while let Some(s) = self.sink.try_pull_sample(gst::ClockTime::ZERO) {
            *self.latest.lock() = s.buffer_owned();
        }
        self.answered()
    }

    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
}

impl Gl {
    /// Stop the pipeline now, on this thread, for a caller that may wait:
    /// the GL probe and the preview strip, which run on a worker. A process
    /// that ends right after must not have a GL context still going down on
    /// a thread of its own.
    pub fn close(self) {
        self.closed.store(true, Ordering::Release);
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

impl Drop for Gl {
    fn drop(&mut self) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let pipeline = self.pipeline.clone();
        let _ = std::thread::Builder::new().name("gmx-fx-gl-stop".into()).spawn(move || {
            let _ = pipeline.set_state(gst::State::Null);
        });
    }
}

#[path = "gl_frames.rs"]
mod frames;
