//! The armed scene, composited beside the mosaic.
//!
//! An operator arms a scene before taking it, and wants to see it. That is the
//! whole of preview, and the question the design had to answer is where the
//! picture comes from. Not the programme pipeline: a preview problem must
//! never reach air, and the README's third isolation boundary says so. Not a
//! pipeline of its own either, because the pictures are already here, once, as
//! the mosaic's per source thumbnails.
//!
//! So the preview is a second `compositor` inside the multiview pipeline, fed
//! from the same thumbnail branches the tiles are drawn from:
//!
//! ```text
//!   source thumb ==> proxysrc > q > rate > scale > caps > tee =|=> mosaic pad
//!                                                              |
//!                                                              +=> q > preview pad
//!                                                                        |
//!   preview compositor  <------------------------------------------------+
//!        |
//!        +--> caps --> jpegenc --> appsink   (/mjpeg/preview, scene.preview.frame)
//!        +--> its own tile on the mosaic
//! ```
//!
//! # What it costs
//!
//! A compositor pad and a leaky queue per source in the armed scene, at
//! thumbnail size. 11 section 3 prices a mosaic sized composite at about 1.7
//! percent of a programme one. Nothing exists while nothing is armed and
//! nobody is subscribed, which is the rule the whole core is built on.
//!
//! # Why a preview cannot touch programme
//!
//! Every branch here hangs off a `tee` carrying `allow-not-linked` inside the
//! *multiview* pipeline, two proxy boundaries away from the compositor that
//! makes the programme. The worst a preview can do is stop reading its own
//! queue, which is leaky downstream, so it loses its own frames. The test
//! kills the preview branch mid stream and watches the programme's frame
//! interval not move.

use crate::caps::CanvasCaps;
use crate::gstutil::{self, make};
use crate::state::SourceId;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// What the preview is composited at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewShape {
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    /// True for `ext.preview = "full"`: the canvas's own size, so a designer's
    /// handles and a projector land on real coordinates.
    pub full: bool,
}

impl PreviewShape {
    /// Mosaic sized, which is what `ext.preview {fps, width}` asks for.
    pub fn at(width: i32, height: i32, fps: i32) -> PreviewShape {
        PreviewShape { width: width.max(16), height: height.max(16), fps: fps.max(1), full: false }
    }

    /// Canvas sized, for `ext.preview = "full"`.
    ///
    /// The pictures in it are still the thumbnail ends: copying every source
    /// into a second pipeline at canvas size is exactly the cost the mosaic
    /// exists to avoid, and a full detail look at one source is a projector on
    /// that source. What `full` buys is geometry: an item at x 1520 is drawn
    /// at x 1520, so a designer's handles need no arithmetic and a projector
    /// fills a screen at the shape the programme will have.
    pub fn full(canvas: &CanvasCaps, fps: i32) -> PreviewShape {
        PreviewShape { width: canvas.width, height: canvas.height, fps: fps.max(1), full: true }
    }
}

/// One source drawn in the preview.
struct Slot {
    source: SourceId,
    /// The tee pad on the tile branch this slot took.
    tee_pad: gst::Pad,
    tee: gst::Element,
    queue: gst::Element,
    pad: gst::Pad,
}

/// The preview compositor and everything hanging off it.
pub struct ScenePreview {
    pipeline: gst::Pipeline,
    comp: gst::Element,
    /// comp, caps, tee, queue, videoconvert, jpegenc, appsink.
    chain: Vec<gst::Element>,
    slots: Vec<Slot>,
    shape: PreviewShape,
    /// The pad the preview takes on the mosaic, so the operator sees it beside
    /// the cameras. `None` until the mosaic makes room for it.
    tile_pad: Option<gst::Pad>,
}

impl ScenePreview {
    /// Build the preview branch into a multiview pipeline that is running.
    ///
    /// Everything is added at once and brought up together, so there is no
    /// moment where a compositor with no sink pad is waiting on a timeout.
    pub fn build(
        pipeline: &gst::Pipeline,
        shape: PreviewShape,
        publish: impl Fn(Arc<[u8]>) + Send + Sync + 'static,
        quality: i32,
    ) -> Result<ScenePreview> {
        let comp = gstutil::make_live_aggregator("compositor", "pv-comp")?;
        comp.set_property_from_str("background", "black");
        crate::probe::set_bool(&comp, "ignore-inactive-pads", true);
        comp.set_property_from_str("start-time-selection", "first");

        let caps = gstutil::capsfilter(
            "pv-caps",
            &CanvasCaps::video_at(shape.width, shape.height, gst::Fraction::new(shape.fps, 1)),
        )?;
        // A tee so the mosaic can take the same picture for the preview's own
        // cell without a second compositor. `allow-not-linked`, so a mosaic
        // that has not taken one is nothing to it.
        let tee = make("tee", "pv-tee")?;
        tee.set_property("allow-not-linked", true);
        let queue = gstutil::queue_preview("pv-q")?;
        let conv = make("videoconvert", "pv-conv")?;
        let enc = make("jpegenc", "pv-jpeg")?;
        crate::probe::set_int(&enc, "quality", quality as i64);
        let sink = gst_app::AppSink::builder()
            .name("pv-sink")
            .max_buffers(1)
            .drop(true)
            .sync(false)
            .build();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    publish(Arc::from(map.as_slice()));
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        let sink: gst::Element = sink.upcast();

        let chain = vec![comp.clone(), caps, tee, queue, conv, enc, sink];
        pipeline.add_many(&chain).context("adding the preview branch")?;
        gst::Element::link_many(chain.iter().collect::<Vec<_>>())
            .context("linking the preview branch")?;
        for el in &chain {
            el.sync_state_with_parent().ok();
        }
        info!(?shape, "preview compositor built");
        Ok(ScenePreview { pipeline: pipeline.clone(), comp, chain, slots: Vec::new(), shape, tile_pad: None })
    }

    pub fn shape(&self) -> PreviewShape {
        self.shape
    }

    /// How many sources the preview is drawing. What it is actually paying for.
    pub fn drawn(&self) -> usize {
        self.slots.len()
    }

    /// The tee the mosaic takes the preview's own cell off.
    pub fn output(&self) -> &gst::Element {
        &self.chain[2]
    }

    pub fn set_tile_pad(&mut self, pad: Option<gst::Pad>) {
        self.tile_pad = pad;
    }

    pub fn tile_pad(&self) -> Option<&gst::Pad> {
        self.tile_pad.as_ref()
    }

    /// Draw a layout: bind what is missing, write every pad, drop what is no
    /// longer in the scene.
    ///
    /// `cells` are in canvas pixels; they are scaled to the preview's own size
    /// here, so a caller never has to know what that is. `tee_for` hands back
    /// the tile tee of a source, or `None` for a source the mosaic does not
    /// carry, which is skipped rather than drawn as a black rectangle.
    pub fn apply(
        &mut self,
        canvas: &CanvasCaps,
        cells: &[Cell],
        tee_for: impl Fn(&SourceId) -> Option<gst::Element>,
    ) -> Result<()> {
        let sx = self.shape.width as f64 / canvas.width.max(1) as f64;
        let sy = self.shape.height as f64 / canvas.height.max(1) as f64;
        let mut kept: Vec<usize> = Vec::with_capacity(cells.len());
        for (z, cell) in cells.iter().enumerate() {
            let index = match self.pick(&cell.source, &kept) {
                Some(index) => index,
                None => {
                    let Some(tee) = tee_for(&cell.source) else {
                        debug!(source = %cell.source, "the mosaic has no tile for a previewed source");
                        continue;
                    };
                    match self.bind(&cell.source, &tee) {
                        Ok(index) => index,
                        Err(e) => {
                            warn!(source = %cell.source, ?e, "could not draw a source in the preview");
                            continue;
                        }
                    }
                }
            };
            let pad = &self.slots[index].pad;
            set_i32(pad, "xpos", (cell.x as f64 * sx).round() as i32);
            set_i32(pad, "ypos", (cell.y as f64 * sy).round() as i32);
            set_i32(pad, "width", ((cell.width as f64 * sx).round() as i32).max(1));
            set_i32(pad, "height", ((cell.height as f64 * sy).round() as i32).max(1));
            pad.set_property("alpha", cell.alpha.clamp(0.0, 1.0));
            pad.set_property("zorder", z as u32);
            kept.push(index);
        }
        let stale: Vec<usize> =
            (0..self.slots.len()).filter(|i| !kept.contains(i)).rev().collect();
        for index in stale {
            self.unbind(index);
        }
        Ok(())
    }

    fn pick(&self, source: &SourceId, kept: &[usize]) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| &s.source == source && !kept.contains(&self.index_of(s)))
    }

    fn index_of(&self, slot: &Slot) -> usize {
        self.slots.iter().position(|s| s.pad == slot.pad).unwrap_or(usize::MAX)
    }

    /// Take a branch off a tile's tee and give it a pad on the preview.
    fn bind(&mut self, source: &SourceId, tee: &gst::Element) -> Result<usize> {
        let queue = gstutil::queue_preview(&format!("pv-q-{source}"))?;
        self.pipeline.add(&queue).context("adding a preview slot")?;
        let tee_pad = tee
            .request_pad_simple("src_%u")
            .with_context(|| format!("the tile tee of {source} refused a pad"))?;
        let sink = queue.static_pad("sink").context("a preview queue has no sink pad")?;
        if let Err(e) = tee_pad.link(&sink) {
            tee.release_request_pad(&tee_pad);
            let _ = self.pipeline.remove(&queue);
            return Err(e.into());
        }
        let pad = self
            .comp
            .request_pad_simple("sink_%u")
            .context("the preview compositor refused a pad")?;
        pad.set_property_from_str("sizing-policy", "keep-aspect-ratio");
        pad.set_property("alpha", 0.0f64);
        queue
            .static_pad("src")
            .context("a preview queue has no src pad")?
            .link(&pad)
            .context("linking a preview slot into the compositor")?;
        queue.sync_state_with_parent().ok();
        self.slots.push(Slot {
            source: source.clone(),
            tee_pad,
            tee: tee.clone(),
            queue,
            pad,
        });
        debug!(%source, "a source joined the preview");
        Ok(self.slots.len() - 1)
    }

    /// Give one slot back: the tile tee first, then the compositor, then the
    /// queue between them.
    ///
    /// The order of the last two is the whole of it. Releasing a compositor's
    /// sink pad flushes it, which wakes anything blocked pushing into it, and
    /// a `queue` taken to NULL has to join its own streaming thread before it
    /// can answer. That thread is inside the compositor's chain function
    /// waiting for a frame the compositor has not taken yet, and it waits
    /// until the pad is flushed. Done the other way round, a preview that has
    /// not produced its first frame holds the mixer loop for ever, which is
    /// what a Linux runner with no GPU and software conversion showed and a
    /// Mac never did.
    fn unbind(&mut self, index: usize) {
        if index >= self.slots.len() {
            return;
        }
        let slot = self.slots.remove(index);
        if let Some(sink) = slot.queue.static_pad("sink") {
            let _ = slot.tee_pad.unlink(&sink);
        }
        slot.tee.release_request_pad(&slot.tee_pad);
        self.comp.release_request_pad(&slot.pad);
        // Locked first, so the bin's own state walk cannot put it back to
        // PLAYING before the remove. See `Encoder::detach`.
        slot.queue.set_locked_state(true);
        let _ = slot.queue.set_state(gst::State::Null);
        let _ = self.pipeline.remove(&slot.queue);
        debug!(source = %slot.source, "a source left the preview");
    }

    /// Stop drawing one source, because its tile is going.
    ///
    /// A slot holds a pad on the tile's tee, so it has to let go before the
    /// tile branch is taken to NULL. Nothing else has to happen: the next
    /// apply binds it again if the armed scene still names it.
    pub fn drop_source(&mut self, source: &SourceId) {
        while let Some(index) = self.slots.iter().position(|s| &s.source == source) {
            self.unbind(index);
        }
    }

    /// Stop the branch dead, for the test that proves a preview cannot reach
    /// air. Everything from the compositor down goes to NULL with buffers
    /// still arriving at its pads.
    pub fn break_for_a_test(&self) {
        for el in &self.chain {
            let _ = el.set_state(gst::State::Null);
        }
    }

    /// Take the whole branch out. Called when the last preview client goes and
    /// when the mosaic goes.
    pub fn teardown(&mut self) {
        while !self.slots.is_empty() {
            self.unbind(self.slots.len() - 1);
        }
        for el in &self.chain {
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
            let _ = self.pipeline.remove(el);
        }
        self.chain.clear();
        info!("preview compositor taken down");
    }
}

/// One item of the armed scene, in canvas pixels.
///
/// The same five numbers `scene::server::PreviewCell` carries, spelled here so
/// that nothing in the multiview pipeline depends on the scene server.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub source: SourceId,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub alpha: f64,
}

fn set_i32(pad: &gst::Pad, name: &str, v: i32) {
    if pad.property::<i32>(name) != v {
        pad.set_property(name, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_preview_is_the_canvas_and_a_mosaic_one_is_not() {
        let canvas = CanvasCaps::new(&Default::default());
        let full = PreviewShape::full(&canvas, 8);
        assert!(full.full);
        assert_eq!((full.width, full.height), (canvas.width, canvas.height));
        let small = PreviewShape::at(480, 270, 8);
        assert!(!small.full);
        assert_eq!((small.width, small.height), (480, 270));
        // Nothing may be built at nothing: a client asking for a zero wide
        // preview gets the floor rather than a compositor that cannot caps.
        assert_eq!(PreviewShape::at(0, 0, 0).width, 16);
        assert_eq!(PreviewShape::at(0, 0, 0).fps, 1);
    }
}
