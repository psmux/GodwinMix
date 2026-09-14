//! Filters: a bin dropped between two elements that both speak the canvas
//! contract.
//!
//! The audit found four raw, canvas normalised insertion points and they are
//! the four this module knows about. A filter does not need a new pipeline
//! design; it needs somewhere to sit and a way in and out while the programme
//! keeps running.
//!
//! ```text
//!  per source, input side                per source, programme side
//!  {id}-vcaps -> [FILTER] -> {id}-vtee   pgm-vq-{id} -> [FILTER] -> vmix:sink_%u
//!  {id}-acaps -> [FILTER] -> {id}-aproxy pgm-amute   -> [FILTER] -> amix:sink_%u
//!
//!  programme, every consumer             programme, output only
//!  vmix-caps -> [FILTER] -> vraw-tee     venc-q -> [FILTER] -> venc-conv
//!  pgm-level -> [FILTER] -> araw-tee     aenc-q -> [FILTER] -> aenc-conv
//! ```
//!
//! Inserting at build time is a link like any other. Inserting live blocks the
//! src pad of the element above the point, relinks underneath the block, and
//! lets go: the same `with_pad_blocked` the outputs already use for a proxy
//! swap, and measured at no more than one frame of interval on the programme.

use super::{Configure, Manifest};
use crate::caps::CanvasCaps;
use crate::config::Params;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::time::Duration;

/// How long to wait for the pad to block before giving up on a live insert.
/// The same figure `output.rs` uses for a proxy swap.
const BLOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// Which stream a filter works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Video,
    Audio,
}

/// Which of the four insertion points a filter sits at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterSide {
    /// On one source, before the proxy boundary. The programme and the
    /// thumbnail both see it.
    SourceInput,
    /// On one source's programme branch only.
    SourceProgramme,
    /// On the programme, before the tee that every consumer reads.
    Programme,
    /// On the programme, on the encoder's branch only. The multiview is
    /// untouched.
    ProgrammeOutput,
}

impl FilterSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceInput => "source-input",
            Self::SourceProgramme => "source-programme",
            Self::Programme => "programme",
            Self::ProgrammeOutput => "programme-output",
        }
    }
}

/// What a filter is asked to build, and where.
#[derive(Debug, Clone)]
pub struct FilterSpec {
    pub id: String,
    pub type_id: String,
    pub side: FilterSide,
    pub params: Params,
}

/// A filter transforms raw media in place. It receives and returns the canvas
/// caps, so it can be put at any of the four points without asking what is
/// upstream.
pub trait Filter: Send {
    fn manifest(&self) -> &Manifest;

    /// Build the bin. It must have a `sink` and a `src` ghost pad and must not
    /// change the caps.
    fn build(&mut self, canvas: &CanvasCaps, params: &Params) -> Result<gst::Element>;

    /// Change a setting on the running bin.
    fn configure(&mut self, params: &Params) -> Result<Configure>;

    /// What the filter adds to the path, in milliseconds. Declared so the
    /// harness can check it against what it measures.
    fn latency_ms(&self) -> u32 {
        0
    }

    /// Which stream this filter belongs on.
    fn stream(&self) -> Stream {
        Stream::Video
    }
}

/// What sits below an insertion point.
///
/// Two of the four points end at an element and two end at a mixer's request
/// pad, so the filter machinery takes either. A pad cannot be reached with
/// `Element::link`, and a request pad must not be released and asked for again
/// just to put something in front of it: that would lose the pad's placement
/// and its offset, which is the whole of where a source sits on the programme.
#[derive(Clone)]
pub enum Downstream {
    Element(gst::Element),
    Pad(gst::Pad),
}

impl Downstream {
    fn name(&self) -> String {
        match self {
            Self::Element(e) => e.name().to_string(),
            Self::Pad(p) => p.name().to_string(),
        }
    }

    fn unlink_from(&self, src: &gst::Pad) {
        match self {
            Self::Element(e) => {
                if let Some(sink) = e.static_pad("sink") {
                    let _ = src.unlink(&sink);
                }
            }
            Self::Pad(p) => {
                let _ = src.unlink(p);
            }
        }
    }

    fn link_from(&self, src: &gst::Pad) -> Result<()> {
        let sink = match self {
            Self::Element(e) => e.static_pad("sink").context("no sink pad below a filter")?,
            Self::Pad(p) => p.clone(),
        };
        src.link(&sink).with_context(|| format!("linking into {}", self.name()))?;
        Ok(())
    }
}

/// A filter that has been put in a pipeline, and everything needed to take it
/// out again.
pub struct FilterSlot {
    pub spec: FilterSpec,
    pub bin: gst::Element,
    /// The element above the insertion point, whose src pad is blocked for an
    /// insert or a removal.
    upstream: gst::Element,
    /// What the upstream element was linked to before this filter arrived.
    downstream: Downstream,
    pipeline: gst::Pipeline,
    filter: Box<dyn Filter>,
}

impl FilterSlot {
    pub fn id(&self) -> &str {
        &self.spec.id
    }

    pub fn configure(&mut self, params: &Params) -> Result<Configure> {
        let outcome = self.filter.configure(params)?;
        if outcome == Configure::Applied {
            self.spec.params = params.clone();
        }
        Ok(outcome)
    }

    /// Put this filter back where it came from: unlink it, relink the two
    /// elements it sat between, and take its bin out of the pipeline.
    ///
    /// Blocked on the same pad the insert blocked, so the programme sees at
    /// most the one frame the block holds.
    pub fn remove(self) -> Result<()> {
        let src = self
            .upstream
            .static_pad("src")
            .context("the element above a filter has no src pad")?;
        let (bin, down, up) = (self.bin.clone(), self.downstream.clone(), self.upstream.clone());
        let src_for_relink = src.clone();
        crate::gstutil::with_pad_blocked(&src, BLOCK_TIMEOUT, move || {
            up.unlink(&bin);
            if let Some(out) = bin.static_pad("src") {
                down.unlink_from(&out);
            }
            if let Err(e) = down.link_from(&src_for_relink) {
                tracing::warn!(?e, "could not relink around a removed filter");
            }
        })
        .context("removing a filter while blocked")?;
        let _ = self.bin.set_state(gst::State::Null);
        let _ = self.pipeline.remove(&self.bin);
        tracing::info!(filter = %self.spec.id, side = self.spec.side.as_str(), "filter removed");
        Ok(())
    }
}

/// Where a filter goes: the element above the point and the one below it.
pub struct Insertion<'a> {
    pub pipeline: &'a gst::Pipeline,
    pub upstream: &'a gst::Element,
    pub downstream: Downstream,
}

impl<'a> Insertion<'a> {
    /// The two insertion points that end at an element.
    pub fn between(
        pipeline: &'a gst::Pipeline,
        upstream: &'a gst::Element,
        downstream: &gst::Element,
    ) -> Self {
        Self { pipeline, upstream, downstream: Downstream::Element(downstream.clone()) }
    }

    /// The two that end at a mixer's request pad.
    pub fn before_pad(
        pipeline: &'a gst::Pipeline,
        upstream: &'a gst::Element,
        pad: &gst::Pad,
    ) -> Self {
        Self { pipeline, upstream, downstream: Downstream::Pad(pad.clone()) }
    }
}

/// Build a filter and put it between `upstream` and `downstream`.
///
/// `live` decides how. At build time the two are not linked yet and the filter
/// is simply linked in. On a running pipeline the pad above is blocked, the
/// link is moved, and the block is released.
pub fn insert(
    at: Insertion<'_>,
    spec: FilterSpec,
    mut filter: Box<dyn Filter>,
    canvas: &CanvasCaps,
    live: bool,
) -> Result<FilterSlot> {
    let bin = filter.build(canvas, &spec.params)?;
    at.pipeline.add(&bin).context("adding a filter to the pipeline")?;
    if live {
        let src = at
            .upstream
            .static_pad("src")
            .context("the element above a filter has no src pad")?;
        let (up, down, b) = (at.upstream.clone(), at.downstream.clone(), bin.clone());
        let src_for_link = src.clone();
        // In NULL until it is in place, then brought up with the rest. An
        // element added to a playing pipeline and linked before it is synced
        // pushes buffers at a bin that is not ready for them.
        crate::gstutil::with_pad_blocked(&src, BLOCK_TIMEOUT, move || {
            down.unlink_from(&src_for_link);
            let joined = up
                .link(&b)
                .map_err(anyhow::Error::from)
                .and_then(|_| match b.static_pad("src") {
                    Some(out) => down.link_from(&out),
                    None => Err(anyhow::anyhow!("the filter bin has no src pad")),
                });
            if let Err(e) = joined {
                tracing::warn!(?e, "could not link a filter in");
            }
        })
        .context("inserting a filter while blocked")?;
        bin.sync_state_with_parent().ok();
    } else {
        at.downstream.unlink_from(&src_at_build(at.upstream)?);
        at.upstream.link(&bin).context("linking a filter to what is above it")?;
        let out = bin.static_pad("src").context("the filter bin has no src pad")?;
        at.downstream.link_from(&out).context("linking a filter to what is below it")?;
    }
    tracing::info!(
        filter = %spec.id,
        kind = %spec.type_id,
        side = spec.side.as_str(),
        live,
        "filter inserted"
    );
    Ok(FilterSlot {
        spec,
        bin,
        upstream: at.upstream.clone(),
        downstream: at.downstream.clone(),
        pipeline: at.pipeline.clone(),
        filter,
    })
}

/// The src pad of the element above an insertion point, at build time.
fn src_at_build(upstream: &gst::Element) -> Result<gst::Pad> {
    upstream.static_pad("src").context("the element above a filter has no src pad")
}

/// Every filter this build ships, by provide id.
pub fn make(type_id: &str) -> Result<Box<dyn Filter>> {
    match type_id {
        t if super::filters::chroma::MANIFEST.is(t) => {
            Ok(Box::new(super::filters::chroma::ChromaKey::default()))
        }
        other => anyhow::bail!(
            "no filter type `{other}` in this build. It has: {}",
            available().join(", ")
        ),
    }
}

pub fn available() -> Vec<String> {
    vec![super::filters::chroma::MANIFEST.provide_id()]
}
