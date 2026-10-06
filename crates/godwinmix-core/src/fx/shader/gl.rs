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
//! answer, and while the answers are more than `STALE` frames behind, it
//! draws the shader's software version (or a dissolve): on a macOS runner
//! the GPU answered once and then not again inside the transition, and the
//! window showed only the old scene. The appsrc keeps one frame and drops
//! the older, so a slow GPU works on the newest and never builds a backlog.

use super::super::frame::Pic;
use crate::overlay::blend::Planes;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    /// Frames sent since the last answer came back.
    waiting: AtomicU32,
}

/// How many frames behind the newest answer may be before the fallback
/// draws instead: a tenth of a second at 30 fps.
const STALE: u32 = 3;

impl Gl {
    /// Build the pipeline and start it. Called off every streaming thread,
    /// because building it loads the GL plugin.
    pub fn start(fragment: &str, size: (i32, i32)) -> Result<Gl> {
        gst::init().context("GStreamer would not start")?;
        let stacked = gst_video::VideoInfo::builder(gst_video::VideoFormat::I420, size.0 as u32, size.1 as u32 * 2)
            .fps(gst::Fraction::new(30, 1))
            .build()
            .context("the stacked frame's layout")?;
        let desc = "appsrc name=src format=time is-live=false do-timestamp=false max-buffers=1 block=false leaky-type=downstream \
                    ! glupload ! glcolorconvert ! glshader name=shader ! glcolorconvert ! gldownload \
                    ! video/x-raw,format=I420 ! appsink name=sink sync=false max-buffers=2 drop=true";
        let pipeline = gst::parse::launch(desc)?.downcast::<gst::Pipeline>().map_err(|_| anyhow::anyhow!("not a pipeline"))?;
        let src = pipeline.by_name("src").context("no appsrc")?.downcast::<gst_app::AppSrc>().map_err(|_| anyhow::anyhow!("not an appsrc"))?;
        let sink = pipeline.by_name("sink").context("no appsink")?.downcast::<gst_app::AppSink>().map_err(|_| anyhow::anyhow!("not an appsink"))?;
        let shader = pipeline.by_name("shader").context("no glshader")?;
        src.set_caps(Some(&stacked.to_caps()?));
        shader.set_property("fragment", fragment);
        let ratio = size.0 as f32 / size.1.max(1) as f32;
        let gl = Gl { pipeline, src, sink, shader, stacked, latest: Mutex::new(None), failed: Arc::default(), ratio, closed: AtomicBool::new(false), waiting: AtomicU32::new(0) };
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
    /// Before the first answer, while the answers are stale, and after the
    /// GPU has failed, `fallback` draws the frame; with none it is the newest
    /// answer, else the old picture, then a dissolve.
    pub fn mix(&self, old: &Pic<'_>, f: &mut Planes<'_>, t: f64, fallback: Option<&dyn super::super::Mix>) {
        if self.failed.load(Ordering::Acquire) || self.error().is_some() {
            return match fallback {
                Some(m) => m.mix(old, f, t),
                None => super::super::matte::dissolve(old, f, t),
            };
        }
        let stacked = frames::stack(&self.stacked, old, f);
        self.answered_now();
        let fresh = self.waiting.load(Ordering::Relaxed) <= STALE;
        let answer = self.latest.lock().clone().filter(|_| fresh || fallback.is_none());
        match (answer, fallback) {
            (Some(answer), _) => frames::draw(&self.stacked, &answer, f),
            (None, Some(m)) => m.mix(old, f, t),
            (None, None) => super::super::matte::dissolve(old, f, 0.0),
        }
        if let Some(buffer) = stacked {
            self.uniforms(t);
            let _ = self.src.push_buffer(buffer);
            self.waiting.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[path = "gl_answers.rs"]
mod answers;

#[path = "gl_frames.rs"]
mod frames;
