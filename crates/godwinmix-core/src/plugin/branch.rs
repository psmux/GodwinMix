//! The programme side branch: the operator's desk for one source.
//!
//! This was 189 lines inlined in `Mixer::add_source_kind`, with 34 property
//! writes and 20 element constructions, and the audit called it "not a type a
//! plugin could be handed". It is a type now. The mixer builds one and hands it
//! to every source, whatever kind it is and whatever tier it runs at.
//!
//! ```text
//!   input pipeline            programme pipeline
//!   {id}-vproxy  ===>  pgm-vsrc-{id} -> pgm-vq-{id} -----------> vmix:sink_%u
//!   {id}-aproxy  ===>  pgm-asrc-{id} -> pgm-aq-{id} -> pgm-again
//!                                        -> pgm-alevel -> pgm-amute -> amix:sink_%u
//! ```
//!
//! The order of the three audio elements is the whole point. The fader is ahead
//! of the meter so that pulling it down visibly pulls the meter down with it: a
//! meter that ignored the fader next to it reads as broken and an operator
//! stops trusting either. The mute is behind the meter so that a muted source
//! still shows its signal, which is what lets someone confirm a camera has
//! sound before cutting to it.

use crate::caps::CanvasCaps;
use crate::gstutil::{self, make};
use crate::state::SourceId;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

/// The loudest a fader can be set to. Matches the ceiling the control plane
/// clamps to, so a request that arrives from somewhere else cannot push a
/// volume element past what the API would have allowed.
pub const MAX_SOURCE_GAIN: f64 = 10.0;

/// The name of a source's meter element. Level messages carry only the name of
/// the element that posted them, so this is how one is attributed back.
pub fn meter_name(id: &str) -> String {
    format!("pgm-alevel-{id}")
}

/// What the mixer hands a source: its side of the proxy boundary, already in
/// the programme pipeline and already on the mixer pads.
pub struct ProgrammeBranch {
    pub id: SourceId,
    /// Every element, in build order, so the mixer can add and remove them as
    /// one and hold a retired branch on air.
    pub elements: Vec<gst::Element>,
    /// The video queue, whose src pad feeds the compositor. Also the per
    /// source programme side filter insertion point.
    pub vq: gst::Element,
    /// The operator's fader, `pgm-again-{id}`.
    pub again: gst::Element,
    /// The operator's mute, `pgm-amute-{id}`. Muted through its `mute`
    /// property rather than by zeroing its volume, so the two controls never
    /// overwrite each other's value.
    pub amute: gst::Element,
    /// The audio queue, for the timeline aligner.
    pub aq: gst::Element,
    pub vpad: gst::Pad,
    pub apad: gst::Pad,
    pub meter: String,
}

/// What the branch needs to know that is not about this source.
pub struct BranchCtx<'a> {
    pub program: &'a gst::Pipeline,
    pub vmix: &'a gst::Element,
    pub amix: &'a gst::Element,
    pub canvas: &'a CanvasCaps,
}

impl ProgrammeBranch {
    /// Build the branch, add it to the programme pipeline and put it on the
    /// mixer pads. The branch arrives invisible and silent: nothing reaches
    /// programme until an operator asks for it.
    pub fn build(
        ctx: &BranchCtx<'_>,
        id: &SourceId,
        video_proxy: &gst::Element,
        audio_proxy: &gst::Element,
        gain: f64,
        muted: bool,
    ) -> Result<Self> {
        let vsrc = make("proxysrc", &format!("pgm-vsrc-{id}"))?;
        vsrc.set_property("proxysink", video_proxy);
        let vq = gstutil::queue_thread(&format!("pgm-vq-{id}"))?;
        let asrc = make("proxysrc", &format!("pgm-asrc-{id}"))?;
        asrc.set_property("proxysink", audio_proxy);
        let aq = gstutil::queue_thread(&format!("pgm-aq-{id}"))?;
        // The programme's latency must not depend on the state of a source's
        // own pipeline. See `answer_latency_here`.
        gstutil::answer_latency_here(&vsrc)?;
        gstutil::answer_latency_here(&asrc)?;

        // The audiomixer sink pad's own `volume` is left out of this chain.
        // Takes and transitions fade that pad, and two things writing one
        // property fight: whichever wrote last wins, so an operator's fader
        // would be undone by the next take, or the take's fade would be undone
        // mid ramp by a fader.
        //
        // No `audioconvert` ahead of the fader. The input pipeline ends its
        // audio branch at a capsfilter on the canvas format, so what arrives
        // through the proxy is already what the audiomixer wants, and both
        // `volume` and `level` take it as it is.
        let again = make("volume", &format!("pgm-again-{id}"))?;
        again.set_property("volume", gain.clamp(0.0, MAX_SOURCE_GAIN));
        let meter = meter_name(id);
        let alevel = make("level", &meter)?;
        crate::probe::set_bool(&alevel, "post-messages", true);
        crate::probe::set_int(&alevel, "interval", 100_000_000);
        let amute = make("volume", &format!("pgm-amute-{id}"))?;
        amute.set_property("mute", muted);

        let elements = vec![
            vsrc,
            vq.clone(),
            asrc,
            aq.clone(),
            again.clone(),
            alevel,
            amute.clone(),
        ];
        ctx.program.add_many(&elements).context("adding source branch")?;
        gst::Element::link_many([&elements[0], &elements[1]]).context("linking source video")?;
        gst::Element::link_many([&elements[2], &elements[3], &elements[4], &elements[5], &elements[6]])
            .context("linking source audio")?;

        let vpad = ctx.vmix.request_pad_simple("sink_%u").context("compositor refused a pad")?;
        vpad.set_property("zorder", 1u32);
        // New sources arrive invisible and silent. Nothing reaches program
        // until an operator asks for it.
        vpad.set_property("alpha", 0.0f64);
        vpad.set_property("xpos", 0i32);
        vpad.set_property("ypos", 0i32);
        vpad.set_property("width", ctx.canvas.width);
        vpad.set_property("height", ctx.canvas.height);
        vpad.set_property_from_str("sizing-policy", "keep-aspect-ratio");
        vq.static_pad("src")
            .context("the video queue has no src pad")?
            .link(&vpad)
            .context("linking video into mixer")?;

        let apad = ctx.amix.request_pad_simple("sink_%u").context("mixer refused a pad")?;
        apad.set_property("volume", 0.0f64);
        // The mute is the last thing before the mixer, so it is what links in.
        amute
            .static_pad("src")
            .context("the mute has no src pad")?
            .link(&apad)
            .context("linking audio into mixer")?;

        Ok(Self { id: id.clone(), elements, vq, again, amute, aq, vpad, apad, meter })
    }

    /// Bring every element up to the pipeline's state.
    pub fn sync_state(&self) {
        for el in &self.elements {
            el.sync_state_with_parent().ok();
        }
    }

    /// Where the fader is now, read off the element. A NaN would have silenced
    /// the element for good, which is why the setter refuses one.
    pub fn gain(&self) -> f64 {
        self.again.property::<f64>("volume")
    }

    pub fn set_gain(&self, gain: f64) {
        if gain.is_nan() {
            // A volume element set to NaN goes silent permanently and logs
            // nothing. The control plane already refuses this; belt and braces
            // for any other caller.
            tracing::warn!(source = %self.id, "ignoring a fader value that is not a number");
            return;
        }
        self.again.set_property("volume", gain.clamp(0.0, MAX_SOURCE_GAIN));
    }

    pub fn muted(&self) -> bool {
        self.amute.property::<bool>("mute")
    }

    pub fn set_muted(&self, muted: bool) {
        self.amute.set_property("mute", muted);
    }

    /// True when this branch's meter posted the level message named.
    ///
    /// Matched whole: a source id may contain a hyphen, so splitting the name
    /// would attribute `pgm-alevel-cam-1` to a source called `cam`.
    pub fn owns_meter(&self, element_name: &str) -> bool {
        self.meter == element_name
    }
}
