//! The pipeline every kind drawn by the overlay board shares: the carrier on
//! the video side, silence on the audio side, and the layer handed to the
//! core. See `overlay` for why such a kind still has a pipeline at all.

use super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::caps::CanvasCaps;
use crate::gstutil::make;
use crate::overlay::carrier::Carrier;
use crate::overlay::Layer;
use crate::plugin::MediaEnds;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// A live silent audio source at the canvas format, for a kind with nothing
/// to say.
pub fn silence(id: &str, canvas: &CanvasCaps) -> Result<gst::Element> {
    let silence = make("audiotestsrc", &format!("{id}-silence"))?;
    crate::probe::set_enum(&silence, "wave", "silence");
    silence.set_property("is-live", true);
    crate::probe::set_int(&silence, "samplesperbuffer", i64::from(canvas.sample_rate / 100));
    Ok(silence)
}

/// Build the carrier and silence into a source pipeline whose layer is
/// `layer`.
pub fn assemble_layer(ctx: &BuildCtx, thumb: bool, carrier: &Carrier, layer: &Arc<Layer>) -> Result<MediaEnds> {
    let silence = silence(&ctx.id, &ctx.canvas)?;
    let mut els = carrier.elements();
    els.push(silence.clone());
    assemble(ctx, thumb, Ingest::default().with(els).livesync(false), |w: &Wiring| {
        carrier.link()?;
        carrier.freeze.link(&w.norm.video_entry()).context("linking the carrier to the canvas")?;
        silence.link(&w.norm.audio_entry()).context("linking the silence")?;
        w.has_video.store(true, Ordering::Relaxed);
        w.has_audio.store(true, Ordering::Relaxed);
        Ok(KindParts { layer: Some(layer.clone()), ..KindParts::default() })
    })
}
