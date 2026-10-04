//! `matte/filter`: the person cut out of the camera picture, with no green or
//! blue screen, so any picture, clip or page can stand behind them.
//!
//! The partner of `chroma/filter`, and drawn the same way: what it hands the
//! board is a `Keyed` picture, the camera frame by reference and an alpha per
//! 2x2 block, so a cutout stacks with every other item, a desk in front of the
//! presenter included. Where a key decides its alpha from the screen colour,
//! this one takes it from a matting model.
//!
//! ```text
//!   frame's thread                      the cutout's thread (worker)
//!   probe: offer(frame) ------------->  sample to the model's size
//!          latest() <--- newest mask    run the model (runtime: GPU or CPU)
//!          blocks + board, as a key     steady it against the last mask
//! ```
//!
//! Every part is its own module and can be changed alone: the settings
//! (`params`), which model and where it is (`model`), the runtime and the
//! device (`runtime`), the thread (`worker`), the picture in and the mask out
//! (`sample`), and the mask laid over a frame (`blocks`, `run`).

mod blocks;
pub mod model;
pub mod params;
pub mod runtime;
mod run;
mod sample;
mod worker;

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
use run::Cutter;
use std::sync::Arc;

pub const MANIFEST: Manifest = Manifest {
    plugin: "matte",
    id: "filter",
    kind: ProvideKind::Filter,
    api: API_LEVEL,
    description: "The person cut out of the camera, with no green screen: a matting model on the \
                  GPU where there is one and the CPU where there is not",
    uri_schemes: &[],
    rank: 128,
    media: MediaDecl {
        video: StreamMode::Raw,
        audio: StreamMode::None,
        alpha: true,
        thumb: false,
    },
    capabilities: CapabilitySet::new(),
    // The edge follows the model, which may answer a frame or two later; the
    // picture itself is never held.
    latency_ms: 0,
    tier: Tier::Core,
};

pub struct Cutout {
    /// Made with the filter, before it is built, as the key's is: the host
    /// may ask for it first.
    hook: BoardHook,
    cutter: Option<Arc<Cutter>>,
    settings: Settings,
}

impl Default for Cutout {
    fn default() -> Self {
        Cutout { hook: BoardHook::new(), cutter: None, settings: Settings::default() }
    }
}

impl Filter for Cutout {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn build(&mut self, _canvas: &CanvasCaps, params: &Params) -> Result<gst::Element> {
        self.settings = Settings::from_params(params)?;
        let cutter = Cutter::new(self.settings.clone(), self.hook.clone());
        static BUILT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = match params.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => format!("matte-{n}"),
        };
        let bin = gst::Bin::with_name(&format!("filter-{name}"));
        let cut = make("identity", &format!("filter-{name}-cut"))?;
        crate::probe::set_bool(&cut, "silent", true);
        bin.add(&cut).context("adding the cutout")?;
        let sink = cut.static_pad("sink").context("the cutout has no sink pad")?;
        let src = cut.static_pad("src").context("the cutout has no src pad")?;
        let c = cutter.clone();
        src.add_probe(gst::PadProbeType::BUFFER, move |pad, info| c.on_buffer(pad, info))
            .context("watching the cutout's frames")?;
        bin.add_pad(&gst::GhostPad::with_target(&sink)?).context("ghosting the sink pad")?;
        bin.add_pad(&gst::GhostPad::with_target(&src)?).context("ghosting the src pad")?;
        self.cutter = Some(cutter);
        Ok(bin.upcast())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        self.settings = Settings::from_params(params)?;
        if let Some(c) = &self.cutter {
            c.set(self.settings.clone());
        }
        Ok(Configure::Applied)
    }

    fn stream(&self) -> Stream {
        Stream::Video
    }

    fn board(&self) -> Option<BoardHook> {
        Some(self.hook.clone())
    }
}

/// A bad param names the field and what it accepts.
pub fn validate(params: &Params) -> Result<()> {
    Settings::from_params(params).map(|_| ())
}

/// Whether a cutout with these settings can run on this machine: the runtime
/// loads and the model it would use is on disk. Asked before one is added, so
/// a missing runtime is a refusal that says what to install rather than a
/// filter that silently cuts nothing out.
pub fn ready(params: &Params) -> Result<String> {
    let s = Settings::from_params(params)?;
    runtime::load()?;
    let gpu = model::wants_gpu(s.device) && runtime::has_gpu(s.device);
    let spec = model::choose(&s.quality, gpu)?;
    Ok(format!("the {} model on the {}", spec.name, if gpu { "GPU" } else { "CPU" }))
}
