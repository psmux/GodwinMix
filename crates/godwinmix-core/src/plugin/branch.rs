//! The programme side branch: the operator's desk for one source.
//!
//! This was 189 lines inlined in `Mixer::add_source_kind`, with 34 property
//! writes and 20 element constructions, and the audit called it "not a type a
//! plugin could be handed". It is a type now. The mixer builds one and hands it
//! to every source, whatever kind it is and whatever tier it runs at.
//!
//! ```text
//!   input pipeline            programme pipeline
//!   {id}-vproxy  ===>  pgm-vsrc-{id} -> pgm-vq-{id} -> pgm-vtee-{id} =|=> slot, slot, ...
//!   {id}-aproxy  ===>  pgm-asrc-{id} -> pgm-aq-{id} -> pgm-again
//!                                        -> pgm-alevel -> pgm-amute -> amix:sink_%u
//! ```
//!
//! The video ends at a `tee` with `allow-not-linked`, not at a compositor pad.
//! A scene may place one source twice (a wide shot and a cut out of the same
//! camera), so the branch offers the picture and the slot pool decides how many
//! places it is drawn in. Audio stays one `audiomixer` pad per source: a source
//! heard twice is not louder.
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
use std::sync::Arc;

/// The loudest a fader can be set to. Matches the ceiling the control plane
/// clamps to, so a request that arrives from somewhere else cannot push a
/// volume element past what the API would have allowed.
pub const MAX_SOURCE_GAIN: f64 = 10.0;

/// The name of a source's meter element. Level messages carry only the name of
/// the element that posted them, so this is how one is attributed back.
pub fn meter_name(id: &str) -> String {
    format!("pgm-alevel-{id}")
}

/// Every compositor pad drawing one source, and the shift that puts that
/// source's timeline on the programme's.
///
/// One source can be on the canvas in several places at once, and each of them
/// is a compositor pad that needs the same offset. Holding them together means
/// the aligner writes the offset once, a pad bound later gets the offset it
/// missed, and nothing has to remember to copy a number from one pad to
/// another.
#[derive(Default)]
pub struct VideoPads {
    inner: parking_lot::Mutex<VideoPadsInner>,
}

#[derive(Default)]
struct VideoPadsInner {
    offset: i64,
    pads: Vec<gst::Pad>,
}

impl VideoPads {
    pub fn new() -> Arc<VideoPads> {
        Arc::new(VideoPads::default())
    }

    /// Start drawing this source on `pad`, with the shift already in force.
    ///
    /// Applied before the pad carries anything: a pad offset adjusts the
    /// segment as it traverses the pad, so setting it after buffers are
    /// flowing changes nothing.
    pub fn attach(&self, pad: &gst::Pad) {
        let mut inner = self.inner.lock();
        pad.set_offset(inner.offset);
        if !inner.pads.iter().any(|p| p == pad) {
            inner.pads.push(pad.clone());
        }
    }

    pub fn detach(&self, pad: &gst::Pad) {
        self.inner.lock().pads.retain(|p| p != pad);
    }

    /// Put this source's timeline at `offset` on every pad drawing it.
    pub fn set_offset(&self, offset: i64) {
        let mut inner = self.inner.lock();
        inner.offset = offset;
        for pad in &inner.pads {
            pad.set_offset(offset);
        }
    }

    pub fn offset(&self) -> i64 {
        self.inner.lock().offset
    }

    /// How many places on the canvas this source is drawn in.
    pub fn count(&self) -> usize {
        self.inner.lock().pads.len()
    }
}

/// What the mixer hands a source: its side of the proxy boundary, already in
/// the programme pipeline and already on the audio mixer pad.
pub struct ProgrammeBranch {
    pub id: SourceId,
    /// Every element, in build order, so the mixer can add and remove them as
    /// one and hold a retired branch on air.
    pub elements: Vec<gst::Element>,
    /// The video queue, whose src pad feeds the tee. Also the per source
    /// programme side filter insertion point.
    pub vq: gst::Element,
    /// Where the picture is offered from. `allow-not-linked`, so a source in
    /// no scene costs nothing and a source in two scenes costs one more slot.
    pub vtee: gst::Element,
    /// Every compositor pad drawing this source, and their shared offset.
    pub pads: Arc<VideoPads>,
    /// The operator's fader, `pgm-again-{id}`.
    pub again: gst::Element,
    /// The operator's mute, `pgm-amute-{id}`. Muted through its `mute`
    /// property rather than by zeroing its volume, so the two controls never
    /// overwrite each other's value.
    pub amute: gst::Element,
    /// The audio queue, for the timeline aligner.
    pub aq: gst::Element,
    pub apad: gst::Pad,
    pub meter: String,
}

/// What the branch needs to know that is not about this source.
pub struct BranchCtx<'a> {
    pub program: &'a gst::Pipeline,
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
        // A flush from the source's pipeline ends here and not at a
        // compositor pad.
        gstutil::stop_flushes_here(&vq)?;
        // `allow-not-linked` is what makes a source in no scene free and a
        // source in two scenes one more slot. Without it, a tee with no branch
        // errors the pipeline the moment the first frame arrives.
        let vtee = make("tee", &format!("pgm-vtee-{id}"))?;
        vtee.set_property("allow-not-linked", true);
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
            vtee.clone(),
            asrc,
            aq.clone(),
            again.clone(),
            alevel,
            amute.clone(),
        ];
        ctx.program.add_many(&elements).context("adding source branch")?;
        gst::Element::link_many([&elements[0], &elements[1], &elements[2]])
            .context("linking source video")?;
        gst::Element::link_many([&elements[3], &elements[4], &elements[5], &elements[6], &elements[7]])
            .context("linking source audio")?;

        let apad = ctx.amix.request_pad_simple("sink_%u").context("mixer refused a pad")?;
        apad.set_property("volume", 0.0f64);
        // The mute is the last thing before the mixer, so it is what links in.
        amute
            .static_pad("src")
            .context("the mute has no src pad")?
            .link(&apad)
            .context("linking audio into mixer")?;

        Ok(Self {
            id: id.clone(),
            elements,
            vq,
            vtee,
            pads: VideoPads::new(),
            again,
            amute,
            aq,
            apad,
            meter,
        })
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
