//! A design that covers the whole picture (`"opaque": true`): a background,
//! a title card, a slate. It has no alpha to keep, so it goes through the
//! compositor like a camera, in its place in the stack, and the overlay board
//! leaves it alone.
//!
//! The renderer sends it whole as I420 (`GMXI`) when it painted. Each frame
//! is pushed into a live `appsrc` stamped on arrival, and the normaliser's
//! rate fills the gaps between them up to the canvas rate. A page that holds
//! still sends nothing, so the last frame is pushed again every
//! `KEEP_ALIVE`, which is what tells the supervisor the source is alive.

use super::super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::caps::CanvasCaps;
use crate::plugin::MediaEnds;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEEP_ALIVE: Duration = Duration::from_millis(250);

pub struct Opaque {
    src: gst_app::AppSrc,
    info: gst_video::VideoInfo,
    last: Mutex<(Option<gst::Buffer>, Instant)>,
    stopped: AtomicBool,
}

impl Opaque {
    /// Pictures of `size`, which the normaliser scales to the canvas when it
    /// is smaller.
    pub fn build(id: &str, canvas: &CanvasCaps, size: (i32, i32)) -> Result<Arc<Opaque>> {
        let caps = CanvasCaps::video_at(size.0, size.1, canvas.fps);
        let info = gst_video::VideoInfo::from_caps(&caps).context("the canvas caps are not a video format")?;
        let src = gst_app::AppSrc::builder()
            .name(format!("{id}-page"))
            .caps(&caps)
            .format(gst::Format::Time)
            .is_live(true)
            .do_timestamp(true)
            .block(false)
            .max_bytes(4 * info.size() as u64)
            .build();
        // A live source that is behind drops its oldest frame, never waits.
        src.set_property_from_str("leaky-type", "downstream");
        Ok(Arc::new(Opaque { src, info, last: Mutex::new((None, Instant::now())), stopped: AtomicBool::new(false) }))
    }

    /// Build it, its pipeline and its keep alive.
    pub fn start(ctx: &BuildCtx, size: (i32, i32), thumb: bool) -> Result<(Arc<Opaque>, MediaEnds)> {
        let o = Opaque::build(&ctx.id, &ctx.canvas, size)?;
        let ends = o.assemble(ctx, thumb)?;
        o.keep_alive(&ctx.id);
        Ok((o, ends))
    }

    /// The source pipeline: these pictures and silence, no layer.
    pub fn assemble(self: &Arc<Self>, ctx: &BuildCtx, thumb: bool) -> Result<MediaEnds> {
        let silence = crate::plugin::kinds::layer::silence(&ctx.id, &ctx.canvas)?;
        let src: gst::Element = self.src.clone().upcast();
        assemble(ctx, thumb, Ingest::default().with([src.clone(), silence.clone()]).livesync(false), |w: &Wiring| {
            src.link(&w.norm.video_entry()).context("linking the page to the canvas")?;
            silence.link(&w.norm.audio_entry()).context("linking the silence")?;
            w.has_video.store(true, Ordering::Relaxed);
            w.has_audio.store(true, Ordering::Relaxed);
            Ok(KindParts::default())
        })
    }

    /// Push the last frame again while the page holds still. Ends when this
    /// is dropped or stopped.
    pub fn keep_alive(self: &Arc<Self>, id: &str) {
        let weak = Arc::downgrade(self);
        let _ = std::thread::Builder::new().name(format!("gmx-html-alive-{id}")).spawn(move || loop {
            std::thread::sleep(KEEP_ALIVE);
            let Some(me) = weak.upgrade() else { return };
            if me.stopped.load(Ordering::Acquire) {
                return;
            }
            let again = {
                let last = me.last.lock();
                last.0.clone().filter(|_| last.1.elapsed() >= KEEP_ALIVE)
            };
            if let Some(b) = again {
                me.push(b);
            }
        });
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
    }

    /// A whole canvas frame in I420, luma then the two chroma planes. False
    /// when it is not the canvas size.
    pub fn show(&self, data: &[u8], width: u32, height: u32) -> bool {
        let info = &self.info;
        if (width, height) != (info.width(), info.height()) || data.len() < (width * height * 3 / 2) as usize {
            return false;
        }
        let Ok(mut buffer) = gst::Buffer::with_size(info.size()) else { return false };
        {
            let Some(buf) = buffer.get_mut() else { return false };
            let Ok(mut frame) = gst_video::VideoFrameRef::from_buffer_ref_writable(buf, info) else { return false };
            let (w, h) = (width as usize, height as usize);
            let planes = [(0usize, w, h), (w * h, w / 2, h / 2), (w * h + (w / 2) * (h / 2), w / 2, h / 2)];
            for (i, (at, pw, ph)) in planes.into_iter().enumerate() {
                let stride = info.stride()[i] as usize;
                let Ok(plane) = frame.plane_data_mut(i as u32) else { return false };
                for row in 0..ph {
                    plane[row * stride..row * stride + pw].copy_from_slice(&data[at + row * pw..at + (row + 1) * pw]);
                }
            }
        }
        self.push(buffer);
        true
    }

    fn push(&self, buffer: gst::Buffer) {
        *self.last.lock() = (Some(buffer.clone()), Instant::now());
        // Never waits: a live source that is behind drops, as a camera would.
        let _ = self.src.push_buffer(buffer);
    }
}
