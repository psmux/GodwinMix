//! The built in source kinds, one file each, over the shared normaliser.
//!
//! Before this module they were five branches of a 386 line function with a
//! chain of `Option<gst::Element>` slots and `.or_else` entry points. Each one
//! is now a type implementing `Source`: it builds whatever sits upstream of the
//! canvas capsfilters, says where its dynamic pads should go, and hands the
//! rest to `assemble`. Nothing in `assemble` knows what a kind is.

pub mod browser;
pub mod exec;
pub mod file;
pub mod layered;
pub mod live;
pub mod normalise;
pub mod rtmp;
pub mod testsrc;

use super::{MediaEnds, Tier};
use crate::caps::CanvasCaps;
use crate::config::{BrowserConfig, SourceConfig};
use crate::input::{install_buffer_probe, install_timeline_probe, route_pads, LastBuffer};
use crate::probe::Backends;
use crate::state::{SourceHealth, SourceId};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use normalise::Normaliser;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

/// Everything a kind needs to build itself. Held by the kind between
/// `initialize` and `start`.
pub struct BuildCtx {
    pub id: SourceId,
    pub cfg: SourceConfig,
    pub canvas: CanvasCaps,
    pub backends: Backends,
    pub thumb_fps: i32,
    pub browser: BrowserConfig,
    pub allow_exec: bool,
    pub origin: Instant,
    pub tier: Tier,
}

/// What a kind contributes upstream of the shared normaliser.
#[derive(Default)]
pub struct Ingest {
    /// Everything to put in the pipeline alongside the normaliser.
    pub elements: Vec<gst::Element>,
    /// Whether this kind's stream wants `livesync`.
    pub livesync: bool,
}

impl Ingest {
    pub fn with(mut self, els: impl IntoIterator<Item = gst::Element>) -> Self {
        self.elements.extend(els);
        self
    }

    pub fn livesync(mut self, on: bool) -> Self {
        self.livesync = on;
        self
    }
}

/// The handles a kind wants the core to hold for it once the pipeline is up.
///
/// The core stores these and never interprets them. They are the parts of the
/// old `InputPipeline` that only ever mattered to one kind.
#[derive(Default)]
pub struct KindParts {
    /// Set for a layered (superimposed) source, which the mixer treats
    /// differently on the timeline and in the supervisor.
    pub superimposed: bool,
    /// What each side of a layered source's compositor has done.
    pub layer_counts: Option<Arc<layered::LayerCounts>>,
    /// The page and media gains, for a source that has separate sounds.
    pub levels: Option<layered::AudioLevels>,
    /// Where a layered source's layers sit in time, one per layer.
    pub placement: Vec<Arc<layered::Placement>>,
    /// Whether a video and an audio pad were ever routed. Filled in by
    /// `assemble`, not by the kind.
    pub has_video: Option<Arc<AtomicBool>>,
    pub has_audio: Option<Arc<AtomicBool>>,
}

/// What `assemble` hands a kind once its elements and the normaliser share one
/// pipeline and the normaliser is linked to itself.
pub struct Wiring<'a> {
    pub ctx: &'a BuildCtx,
    pub pipeline: &'a gst::Pipeline,
    pub norm: &'a Normaliser,
    pub has_video: &'a Arc<AtomicBool>,
    pub has_audio: &'a Arc<AtomicBool>,
}

impl Wiring<'_> {
    /// Route a dynamic element's pads into the normaliser, which is what every
    /// kind with a demuxer or a decodebin wants.
    pub fn route(&self, dynamic: &gst::Element, video: gst::Element, audio: gst::Element) {
        route_pads(
            dynamic,
            &self.ctx.id,
            Some(video),
            Some(audio),
            self.has_video,
            self.has_audio,
        );
    }
}

/// The body shared by `hls/source` and `file/source`: one `uridecodebin`,
/// which already knows how to open a playlist, an RTSP session or a file, and
/// hands over raw media on dynamic pads.
///
/// The two kinds differ in one thing only, and it is not the protocol: a live
/// stream drifts and gaps, so it goes through `livesync`, while a file's
/// timestamps start at zero and are rebased on the mixer pad instead.
pub fn uridecode(ctx: &BuildCtx, thumb: bool, livesync: bool) -> Result<MediaEnds> {
    let el = crate::gstutil::make("uridecodebin", &format!("{}-src-uri", ctx.id))?;
    el.set_property("uri", crate::input::to_uri(&ctx.cfg.uri));
    crate::probe::set_bool(&el, "use-buffering", true);
    assemble(
        ctx,
        thumb,
        Ingest::default().with([el.clone()]).livesync(livesync),
        |w: &Wiring| {
            w.route(&el, w.norm.video_entry(), w.norm.audio_entry());
            Ok(KindParts::default())
        },
    )
}

/// Put a kind's ingest and the shared normaliser in one pipeline, link the
/// normaliser, install the liveness probes, then let the kind wire itself up.
///
/// The order matters and is the order the old function used: probes on the
/// proxy sinks before anything can flow, and the kind's own linking last so
/// that a dynamic pad added during it finds a normaliser already joined.
pub fn assemble<F>(ctx: &BuildCtx, thumb: bool, ing: Ingest, wire: F) -> Result<MediaEnds>
where
    F: FnOnce(&Wiring) -> Result<KindParts>,
{
    let id = &ctx.id;
    let pipeline = gst::Pipeline::with_name(&format!("input-{id}"));
    let norm = Normaliser::build(
        id,
        &ctx.canvas,
        thumb.then_some(ctx.thumb_fps),
        ing.livesync,
    )?;

    let mut all: Vec<&gst::Element> = ing.elements.iter().collect();
    all.extend(norm.elements());
    pipeline.add_many(&all).context("adding input elements")?;
    norm.link()?;

    // --- liveness probes ---------------------------------------------
    // Placed after normalisation so they count frames the mixer can actually
    // use, not frames that arrived and failed to convert. On the proxy sinks,
    // the last thing before the mixer: a probe further upstream reports a
    // source as healthy while an element downstream of it silently discards
    // everything, which is exactly how the cameras appeared to have audio
    // while the programme carried silence.
    let health = SourceHealth::new(ctx.origin);
    install_buffer_probe(&norm.video_proxy, "sink", {
        let h = health.clone();
        move || h.mark_video()
    })?;
    install_buffer_probe(&norm.audio_proxy, "sink", {
        let h = health.clone();
        move || h.mark_audio()
    })?;
    // On the same pads, and for the same reason they are the right pads: this
    // is the last place a buffer can be seen before it crosses to the
    // programme, so it is where its timing means what the compositor will make
    // of it.
    let last_video = Arc::new(LastBuffer::default());
    let last_audio = Arc::new(LastBuffer::default());
    install_timeline_probe(&norm.video_proxy, "sink", &last_video)?;
    install_timeline_probe(&norm.audio_proxy, "sink", &last_audio)?;

    let has_video = Arc::new(AtomicBool::new(false));
    let has_audio = Arc::new(AtomicBool::new(false));
    let mut parts = wire(&Wiring {
        ctx,
        pipeline: &pipeline,
        norm: &norm,
        has_video: &has_video,
        has_audio: &has_audio,
    })?;
    // The two flags belong to the core, not to the kind, so they are filled in
    // here whatever the kind did with them.
    parts.has_video = Some(has_video);
    parts.has_audio = Some(has_audio);

    Ok(MediaEnds {
        pipeline,
        video: norm.video_proxy.clone(),
        audio: norm.audio_proxy.clone(),
        thumb: norm.thumb_proxy(),
        vcaps: norm.vcaps.clone(),
        acaps: norm.acaps.clone(),
        vtee: norm.vtee.clone(),
        health,
        last_video,
        last_audio,
        parts,
    })
}
