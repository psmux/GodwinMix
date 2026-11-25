//! The sound filters: gain, equaliser, gate, expander, compressor, limiter,
//! delay, noise suppression and loudness, each a tier 0 filter of its own.
//!
//! Every one runs in the same probe (`run`), on the streaming thread that
//! carries the samples, with no copy, no thread and no element of its own,
//! which is what lets a Raspberry Pi run a chain on every microphone. Each
//! goes on a source, before or after its fader (`attach.side`), or on the
//! programme, where a limiter keeps the whole show under its ceiling. They
//! read OBS's setting names too, so an imported filter keeps its sound.
//!
//! ```toml
//! [[filters]]
//! id = "pastor-eq"
//! type = "eq/filter"
//! attach = { source = "pulpit-mic" }
//! params = { low_cut_hz = 90, high_gain_db = 2 }
//! ```

mod biquad;
mod delay;
#[cfg(feature = "denoise")]
mod denoise;
mod dynamics;
mod eq;
mod gain;
mod gate;
mod loudness;
pub mod run;
pub(crate) mod values;

use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::plugin::filter::{Filter, Stream};
use crate::plugin::{
    CapabilitySet, Configure, Manifest, MediaDecl, ProvideKind, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use dynamics::Mode;
use gstreamer as gst;
use gstreamer::prelude::*;
use run::{Dsp, Runner};
use std::sync::Arc;

const fn manifest(plugin: &'static str, description: &'static str, latency_ms: u32) -> Manifest {
    Manifest {
        plugin,
        id: "filter",
        kind: ProvideKind::Filter,
        api: API_LEVEL,
        description,
        uri_schemes: &[],
        rank: 128,
        media: MediaDecl { video: StreamMode::None, audio: StreamMode::Raw, alpha: false, thumb: false },
        capabilities: CapabilitySet::new(),
        latency_ms,
        tier: Tier::Core,
    }
}

pub const GAIN: Manifest = manifest("gain", "Turn a source's sound up or down, in decibels", 0);
pub const EQ: Manifest = manifest("eq", "A low cut, bass, middle and treble, and up to ten bands of your own", 0);
pub const GATE: Manifest = manifest("gate", "Silence the room between words", 0);
pub const EXPANDER: Manifest = manifest("expander", "Push quiet background sound further down", 0);
pub const COMPRESSOR: Manifest = manifest("compressor", "Even out loud and quiet speech", 0);
pub const LIMITER: Manifest = manifest("limiter", "Keep the sound under a ceiling, whatever comes in", 0);
pub const DELAY: Manifest = manifest("delay", "Hold the sound back to meet a late picture", 0);
pub const LOUDNESS: Manifest = manifest("loudness", "Ride the level towards a loudness target, in LUFS", 0);
#[cfg(feature = "denoise")]
pub const DENOISE: Manifest = manifest("denoise", "Take fans, hum and traffic out from under a voice", 10);

/// Every sound filter in this build.
pub fn built_in() -> Vec<&'static Manifest> {
    #[cfg_attr(not(feature = "denoise"), allow(unused_mut))]
    let mut out = vec![&GAIN, &EQ, &GATE, &EXPANDER, &COMPRESSOR, &LIMITER, &DELAY, &LOUDNESS];
    #[cfg(feature = "denoise")]
    out.push(&DENOISE);
    out
}

fn stage(type_id: &str) -> Option<(&'static Manifest, Box<dyn Dsp>)> {
    let found: (&'static Manifest, Box<dyn Dsp>) = match type_id {
        t if GAIN.is(t) => (&GAIN, Box::<gain::Gain>::default()),
        t if EQ.is(t) => (&EQ, Box::<eq::Eq>::default()),
        t if GATE.is(t) => (&GATE, Box::<gate::Gate>::default()),
        t if EXPANDER.is(t) => (&EXPANDER, Box::new(dynamics::Dynamics::new(Mode::Expand))),
        t if COMPRESSOR.is(t) => (&COMPRESSOR, Box::new(dynamics::Dynamics::new(Mode::Compress))),
        t if LIMITER.is(t) => (&LIMITER, Box::new(dynamics::Dynamics::new(Mode::Limit))),
        t if DELAY.is(t) => (&DELAY, Box::<delay::Delay>::default()),
        t if LOUDNESS.is(t) => (&LOUDNESS, Box::<loudness::Loudness>::default()),
        #[cfg(feature = "denoise")]
        t if DENOISE.is(t) => (&DENOISE, Box::<denoise::Denoise>::default()),
        _ => return None,
    };
    Some(found)
}

/// The filter for `type_id`, when it is one of the sound filters.
pub fn make_filter(type_id: &str) -> Option<Box<dyn Filter>> {
    let (manifest, dsp) = stage(type_id)?;
    Some(Box::new(AudioFilter { manifest, runner: Arc::new(Runner::new(manifest.provide_id(), dsp)) }))
}

/// True for any of the sound filters, whether or not this build carries it.
pub fn is_audio(type_id: &str) -> bool {
    let plugin = type_id.split('/').next().unwrap_or("");
    type_id.ends_with("/filter")
        && matches!(plugin, "gain" | "eq" | "gate" | "expander" | "compressor" | "limiter" | "delay" | "denoise" | "loudness")
}

/// A bad param names the filter, the field and what it accepts.
pub fn validate(type_id: &str, params: &Params) -> Result<()> {
    stage(type_id).map_or(Ok(()), |(m, dsp)| Runner::new(m.provide_id(), dsp).set(params))
}

pub struct AudioFilter {
    manifest: &'static Manifest,
    runner: Arc<Runner>,
}

impl Filter for AudioFilter {
    fn manifest(&self) -> &Manifest {
        self.manifest
    }

    fn build(&mut self, canvas: &CanvasCaps, params: &Params) -> Result<gst::Element> {
        self.runner.accepts(canvas.sample_rate as u32)?;
        self.runner.set(params)?;
        static BUILT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = match params.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => format!("{}-{n}", self.manifest.plugin),
        };
        let bin = gst::Bin::with_name(&format!("filter-{name}"));
        let stage = make("identity", &format!("filter-{name}-dsp"))?;
        crate::probe::set_bool(&stage, "silent", true);
        bin.add(&stage).context("adding the sound filter")?;
        let sink = stage.static_pad("sink").context("sound filter has no sink pad")?;
        let src = stage.static_pad("src").context("sound filter has no src pad")?;
        let runner = self.runner.clone();
        src.add_probe(gst::PadProbeType::BUFFER, move |pad, info| runner.on_buffer(pad, info))
            .context("watching the sound filter's buffers")?;
        bin.add_pad(&gst::GhostPad::with_target(&sink)?).context("ghosting the sink pad")?;
        bin.add_pad(&gst::GhostPad::with_target(&src)?).context("ghosting the src pad")?;
        Ok(bin.upcast())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        self.runner.set(params)?;
        Ok(Configure::Applied)
    }

    fn latency_ms(&self) -> u32 {
        self.runner.latency_ms()
    }

    fn stream(&self) -> Stream {
        Stream::Audio
    }
}

#[cfg(test)]
mod tests;
