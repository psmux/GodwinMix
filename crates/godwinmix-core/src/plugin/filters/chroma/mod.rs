//! `chroma/filter`: a green or blue screen key, written for this mixer.
//!
//! It replaces a bin of `videoconvert ! alpha ! videoconvert`, which cost
//! two full frame conversions and, worse, lost its alpha on the way out: the
//! canvas is I420, so every keyed pixel reached the compositor as black and
//! the presenter stood on a black box. This key never converts the frame. It
//! reads the I420 it is given and decides each 2x2 block with one lookup in a
//! table built from the settings (`lut`), then hands an AYUV picture of the
//! area the matte keeps to the overlay board, which draws it over the
//! programme where the item sits, in stacking order with every other
//! transparent item. A desk in front of the presenter is a transparent PNG
//! above it, and stays in front.
//!
//! ```text
//!   slot: gate > q > crop > flip > [identity + probe] -> vmix pad (fed gaps)
//!                                         \
//!                                          `-> layer -> board, after vmix
//! ```
//!
//! Where nothing can draw it (a source's input side, or a programme composited
//! on a GPU) the layer stays off and the key is flattened into the frame over
//! black, which is what the old key did on every side.
//!
//! The bin is a single `identity`, so the caps through it are exactly the
//! caps into it and the filter is interchangeable with no filter at all.

mod colour;
mod feather;
pub mod frame;
pub mod guess;
pub mod lut;
pub mod params;
mod run;
#[cfg(test)]
mod sample;
#[cfg(test)]
mod tests;

pub use colour::{parse_hex, rgb_to_yuv, to_hex, yuv_to_rgb};
pub use params::Settings;

use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::plugin::filter::{BoardHook, Filter, Stream};
use crate::plugin::{
    CapabilitySet, Configure, Manifest, MediaDecl, ProvideKind, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use run::Keyer;
use std::sync::Arc;

pub const MANIFEST: Manifest = Manifest {
    plugin: "chroma",
    id: "filter",
    kind: ProvideKind::Filter,
    api: API_LEVEL,
    description: "A green or blue screen key with spill removal, a soft edge and a garbage matte",
    uri_schemes: &[],
    rank: 128,
    media: MediaDecl {
        video: StreamMode::Raw,
        audio: StreamMode::None,
        alpha: true,
        thumb: false,
    },
    capabilities: CapabilitySet::new(),
    // One pass over the frame, inside the frame's own time. Nothing is held.
    latency_ms: 0,
    tier: Tier::Core,
};

pub struct ChromaKey {
    keyer: Arc<Keyer>,
}

impl Default for ChromaKey {
    fn default() -> Self {
        ChromaKey { keyer: Keyer::new(Settings::default()) }
    }
}

impl Filter for ChromaKey {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn build(&mut self, _canvas: &CanvasCaps, params: &Params) -> Result<gst::Element> {
        self.keyer.set(Settings::from_params(params)?);
        // Unique whatever the params say: two keyed items in one scene are
        // two bins in one pipeline, and a bin name is taken only once.
        static BUILT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = match params.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => format!("chroma-{n}"),
        };
        let bin = gst::Bin::with_name(&format!("filter-{name}"));
        let key = make("identity", &format!("filter-{name}-key"))?;
        crate::probe::set_bool(&key, "silent", true);
        bin.add(&key).context("adding the chroma key")?;
        let sink = key.static_pad("sink").context("chroma key has no sink pad")?;
        let src = key.static_pad("src").context("chroma key has no src pad")?;
        let keyer = self.keyer.clone();
        src.add_probe(gst::PadProbeType::BUFFER, move |pad, info| keyer.on_buffer(pad, info))
            .context("watching the chroma key's frames")?;
        bin.add_pad(&gst::GhostPad::with_target(&sink)?).context("ghosting the sink pad")?;
        bin.add_pad(&gst::GhostPad::with_target(&src)?).context("ghosting the src pad")?;
        Ok(bin.upcast())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        self.keyer.set(Settings::from_params(params)?);
        Ok(Configure::Applied)
    }

    fn stream(&self) -> Stream {
        Stream::Video
    }

    fn board(&self) -> Option<BoardHook> {
        Some(self.keyer.hook.clone())
    }
}

/// A bad param names the field and what it accepts.
pub fn validate(params: &Params) -> Result<()> {
    Settings::from_params(params).map(|_| ())
}
