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
        let gl = Gl { pipeline, src, sink, shader, stacked, latest: Mutex::new(None), failed: Arc::default(), ratio };
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
        let stacked = self.stack(old, f);
        while let Some(s) = self.sink.try_pull_sample(gst::ClockTime::ZERO) {
            *self.latest.lock() = s.buffer_owned();
        }
        if let Some(answer) = self.latest.lock().clone() {
            self.draw(&answer, f);
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

    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    /// One buffer twice the canvas height, old picture above new.
    fn stack(&self, old: &Pic<'_>, f: &Planes<'_>) -> Option<gst::Buffer> {
        let mut buffer = gst::Buffer::with_size(self.stacked.size()).ok()?;
        {
            let b = buffer.get_mut()?;
            let mut frame = gst_video::VideoFrameRef::from_buffer_ref_writable(b, &self.stacked).ok()?;
            let (w, h) = (f.width as usize, f.height as usize);
            let s = self.stacked.stride();
            let planes = frame.planes_data_mut();
            let [y, u, v, _] = planes;
            let new = [&*f.y, &*f.u, &*f.v];
            for (i, (dst, rows, cols)) in [(y, h, w), (u, h / 2, w / 2), (v, h / 2, w / 2)].into_iter().enumerate() {
                let ds = s[i] as usize;
                let olds = [old.y, old.u, old.v][i];
                for r in 0..rows {
                    dst[r * ds..r * ds + cols].copy_from_slice(&olds[r * old.strides[i]..][..cols]);
                    dst[(rows + r) * ds..(rows + r) * ds + cols].copy_from_slice(&new[i][r * f.strides[i]..][..cols]);
                }
            }
        }
        Some(buffer)
    }

    /// The top half of an answer onto the frame.
    fn draw(&self, answer: &gst::Buffer, f: &mut Planes<'_>) {
        let Ok(frame) = gst_video::VideoFrameRef::from_buffer_ref_readable(answer.as_ref(), &self.stacked) else { return };
        let (w, h) = (f.width as usize, f.height as usize);
        let s = self.stacked.stride();
        let dst = [(&mut *f.y, f.strides[0], h, w), (&mut *f.u, f.strides[1], h / 2, w / 2), (&mut *f.v, f.strides[2], h / 2, w / 2)];
        for (i, (d, ds, rows, cols)) in dst.into_iter().enumerate() {
            let Ok(src) = frame.plane_data(i as u32) else { return };
            for r in 0..rows {
                d[r * ds..r * ds + cols].copy_from_slice(&src[r * s[i] as usize..][..cols]);
            }
        }
    }
}

impl Drop for Gl {
    fn drop(&mut self) {
        let pipeline = self.pipeline.clone();
        let _ = std::thread::Builder::new().name("gmx-fx-gl-stop".into()).spawn(move || {
            let _ = pipeline.set_state(gst::State::Null);
        });
    }
}
