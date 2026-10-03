//! The slot pool: how a scene reaches the compositor without adding an
//! element.
//!
//! One compositor, a fixed pool of slots, and the scene tree flattened before
//! it gets here. A slot is a chain that never changes shape:
//!
//! ```text
//!   pgm-vtee-{source} =|=> slot-gate-N -> slot-q-N -> slot-crop-N -> slot-flip-N -> vmix:sink_N
//! ```
//!
//! Applying a scene is: flatten, bind each entry to a slot (reusing the slot
//! that already holds that source), write pad properties, alpha 0 the rest. No
//! element is added or removed on the ordinary path, which is the property the
//! mixer's own header calls the whole point of the design: a take is a set of
//! property writes and the encoder cannot tell that anything happened.
//!
//! Three things make that true in practice rather than on paper:
//!
//! * A source is bound to a slot when it is added, not when it is taken. By
//!   the time anybody takes a scene the bindings are already there, so the
//!   take is property writes alone.
//! * The pool starts at eight. A pad at alpha 0 costs nothing (the compositor
//!   skips conversion, scale and blend for it; sixteen hidden pads measured
//!   within 1.5 percent of no compositor at all), so slots nobody is using are
//!   not worth reclaiming.
//! * A cache miss, which is a source placed in more places than it has slots,
//!   relinks under `with_pad_blocked`. The pool makes that rare by
//!   construction; live pad addition is the documented hazard.
//!
//! The `valve` at the head of each slot is what keeps a hidden slot honest. A
//! compositor pad at alpha 0 is skipped, but the chain feeding it is not: the
//! queue still has a thread and the crop and the flip still see every frame,
//! and sixteen of those measured 180 percent over the baseline. A closed valve
//! drops the buffer where the tee hands it over, before any of that, and the
//! same sixteen measure inside the 2 percent budget. One slot per source, the
//! one it was given when it was added, keeps its valve open whatever the scene
//! says, so the ordinary path costs exactly what it did before the pool
//! existed and a take never waits for a frame.
//!
//! z is banded so the slate and the freeze frame keep working:
//!
//! | Band | What is in it |
//! |---|---|
//! | 0 | the slate, permanently, fully opaque |
//! | 1 to 99 | the branches of sources being rebuilt, holding their last frame |
//! | 1000 and up | the live items of the scene, bottom of the stack first |

use crate::caps::CanvasCaps;
use crate::gstutil::{self, make};
use crate::mixer::transition::item::Motion;
use crate::mixer::transition::{CropTarget, Layering, Leg, Rect};
use crate::plugin::branch::ProgrammeBranch;
use crate::plugin::filter::{FilterSide, FilterSlot, FilterSpec, Insertion};
use crate::scene::id::Id;
use crate::state::SourceId;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::time::Duration;
use tracing::{debug, info, warn};

/// How many slots the pool starts with.
///
/// Eight is the number 11 section 3 settles on: more scenes than anyone builds
/// fit in it, and hidden pads are free, so there is no reason to start smaller
/// and every reason not to relink during a show.
pub const INITIAL_SLOTS: usize = 8;

/// The slate's z order. Nothing else is ever put here.
pub const Z_SLATE: u32 = 0;
/// The bottom of the band held for a source being rebuilt.
pub const Z_RETIRED: u32 = 1;
/// The top of that band, so a retired branch can never climb over a live item.
pub const Z_RETIRED_TOP: u32 = 99;
/// The bottom of the live band. An item's index in the flattened scene is
/// added to it, so item 0 is at the back.
pub const Z_LIVE: u32 = 1000;
/// Where the outgoing scene is put for a transition that draws the new one
/// over it: above the retired band, below every live item.
pub const Z_UNDER: u32 = 100;
/// Where the outgoing scene is put for a transition that draws it over the
/// new one. Far enough above the live band that no scene reaches it, and
/// below the stinger's clip, which is at the very top.
pub const Z_OVER: u32 = Z_LIVE + 100_000;

/// How long to wait for a slot's tee pad to reach an idle point on a cache
/// miss. The figure the filters and the proxy swaps already use.
///
/// Five seconds looks far too long for something that happens on the mixer
/// loop, and it was tried at half a second. That made everything worse, which
/// is worth recording here so nobody tries it again. A bind that gives up
/// leaves the slot unbound; the visibility tick reapplies the whole scene
/// twice a second; the next apply tries the same bind again. A short deadline
/// turns one slow pad into half a second of the mixer loop every half second,
/// for ever. Measured over fourteen add and remove cycles against a running
/// programme, removing one `videotestsrc` went from a worst case of 300 ms at
/// five seconds to a worst case of 5.4 s at half a second, and in a soak it
/// ran away to 74 s.
///
/// The right fix is not a shorter deadline. It is to prepare and retire a
/// branch off the mixer loop and keep only the relink inside the blocked
/// interval, which is a larger change than a constant.
const BLOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether the source behind a placement is heard.
///
/// A source is audible when any live item of it says so, which is OBS's
/// behaviour and changes no pad topology: there is still one `audiomixer` pad
/// per source however many places the picture is drawn in, because a source
/// heard twice is not louder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlacementAudio {
    /// Heard while this item is visible.
    #[default]
    Follow,
    /// Heard whether this item is visible or not.
    Always,
    /// Never heard through this item.
    Never,
}

/// How the picture fills its box, in the words the compositor pad has.
///
/// Three, because a `compositor` sink pad has three. The seven `fit` keywords
/// of the document map onto them and the reference page says which is which,
/// rather than the picture quietly being wrong. Each one carries the nicks to
/// try in order: a pad built by an older `compositor` has no
/// `keep-aspect-ratio-with-crop`, and writing a nick an element does not have
/// panics inside glib rather than failing, so the first one the pad actually
/// has is what gets written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sizing {
    /// Scale to the box exactly, aspect ratio and all. What `stretch` means,
    /// and what an item at its own size gets because its box is its own size.
    #[default]
    Fill,
    /// Fit inside the box, letterboxing the rest: `contain` and `max`.
    Contain,
    /// Fill the box and let the overflow go: `cover`, `fit-width`,
    /// `fit-height`.
    Cover,
}

impl Sizing {
    /// The word for it on the wire: `fill`, `contain` or `cover`.
    pub fn name(self) -> &'static str {
        match self {
            Sizing::Fill => "fill",
            Sizing::Contain => "contain",
            Sizing::Cover => "cover",
        }
    }

    /// The other way. Anything unknown is `contain`, which distorts nothing.
    pub fn named(name: &str) -> Sizing {
        match name {
            "fill" => Sizing::Fill,
            "cover" => Sizing::Cover,
            _ => Sizing::Contain,
        }
    }

    /// The nicks to try, best first.
    pub fn nicks(self) -> &'static [&'static str] {
        match self {
            Sizing::Fill => &["none"],
            Sizing::Contain => &["keep-aspect-ratio", "none"],
            Sizing::Cover => &["keep-aspect-ratio-with-crop", "keep-aspect-ratio", "none"],
        }
    }
}

/// One filter an item carries, as the slot chain needs it.
///
/// The document holds these as JSON on the item; `scene::server::compose`
/// turns them into this on the way here, so nothing in the pipeline reads a
/// `serde_json::Value` and nothing in the document knows what a GStreamer
/// element is.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemFilter {
    /// A filter provide id, `chroma/filter`.
    pub type_id: String,
    /// The operator's name for it, when they gave one.
    pub name: Option<String>,
    pub params: crate::config::Params,
}

impl ItemFilter {
    /// What decides whether the chain has to be rebuilt.
    ///
    /// Type and parameters, in that order. Two filters of the same type with
    /// the same parameters are the same filter however they are named, so
    /// renaming one in the designer does not take a pad block on air.
    fn shape(&self) -> (&str, &crate::config::Params) {
        (&self.type_id, &self.params)
    }
}

/// The name a slot's filter is drawn under on the board. Not a slug, so it
/// can never be mistaken for a source id.
fn board_key(index: usize, n: usize) -> String {
    format!("slot {index} filter {n}")
}

/// What a scene wants drawn in one slot.
///
/// Plain numbers in canvas pixels, worked out from the document by
/// `scene::geometry::flatten` before anything here is called. Nothing in this
/// module knows what a scene document is.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub source: SourceId,
    /// The scene item this placement came from, when it came from a document.
    /// What a `move` transition matches on across two scenes, and what keeps a
    /// re-apply on the same pad it was already drawn on.
    pub item: Option<Id>,
    /// Filters over this item alone, innermost first. They live on the slot
    /// chain, above the compositor pad, so the same camera can be keyed in one
    /// item and clean in another.
    pub filters: Vec<ItemFilter>,
    /// The children this placement composites, for a group that carries a
    /// filter and therefore cannot be flattened. Empty for every ordinary
    /// item, which is every item that is not a filtered group. See
    /// `mixer::group`: this is the expensive path and is built on demand.
    pub group: Vec<Placement>,
    pub xpos: i32,
    pub ypos: i32,
    pub width: i32,
    pub height: i32,
    pub alpha: f64,
    /// Fractions of the source's own picture to trim, 0 to 1 each.
    pub crop: (f64, f64, f64, f64),
    /// Degrees clockwise. Snapped to the nearest right angle on the software
    /// path, which is all `videoflip` can do.
    pub rotation: f64,
    pub sizing: Sizing,
    /// 0 to 1 each, where the picture sits inside its frame when the sizing
    /// policy leaves room.
    pub align: (f64, f64),
    pub audio: PlacementAudio,
    /// The item's own `enter` and `exit`, played when it is shown or hidden on
    /// air. Empty for nearly every item.
    pub motion: Motion,
}

impl Placement {
    /// A source filling the whole canvas, which is what a bare source id means.
    pub fn full_canvas(source: SourceId, canvas: &CanvasCaps) -> Placement {
        Placement {
            source,
            item: None,
            filters: Vec::new(),
            group: Vec::new(),
            xpos: 0,
            ypos: 0,
            width: canvas.width,
            height: canvas.height,
            alpha: 1.0,
            crop: (0.0, 0.0, 0.0, 0.0),
            rotation: 0.0,
            sizing: Sizing::Contain,
            align: (0.5, 0.5),
            audio: PlacementAudio::Follow,
            motion: Motion::default(),
        }
    }

    /// Is the source behind this placement heard?
    pub fn heard(&self) -> bool {
        match self.audio {
            PlacementAudio::Always => true,
            PlacementAudio::Follow => self.alpha > 0.0,
            PlacementAudio::Never => false,
        }
    }
}

/// Where one slot's picture sits, as five numbers the compositor reads.
///
/// Everything a geometry command can animate and nothing it cannot: the crop,
/// the rotation and the sizing policy are steps rather than ramps, so they are
/// written once at the start and left alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadState {
    pub xpos: i32,
    pub ypos: i32,
    pub width: i32,
    pub height: i32,
    pub alpha: f64,
}

impl PadState {
    fn read(pad: &gst::Pad) -> PadState {
        PadState {
            xpos: pad.property("xpos"),
            ypos: pad.property("ypos"),
            width: pad.property("width"),
            height: pad.property("height"),
            alpha: pad.property("alpha"),
        }
    }

    fn of(p: &Placement) -> PadState {
        PadState {
            xpos: p.xpos,
            ypos: p.ypos,
            width: p.width.max(0),
            height: p.height.max(0),
            alpha: p.alpha.clamp(0.0, 1.0),
        }
    }

    /// This state a fraction of the way towards another one.
    pub fn lerp(&self, to: &PadState, t: f64) -> PadState {
        let f = |a: i32, b: i32| (a as f64 + (b - a) as f64 * t).round() as i32;
        PadState {
            xpos: f(self.xpos, to.xpos),
            ypos: f(self.ypos, to.ypos),
            width: f(self.width, to.width),
            height: f(self.height, to.height),
            alpha: self.alpha + (to.alpha - self.alpha) * t,
        }
    }

    pub fn write(&self, pad: &gst::Pad) {
        set_i32(pad, "xpos", self.xpos);
        set_i32(pad, "ypos", self.ypos);
        set_i32(pad, "width", self.width.max(1));
        set_i32(pad, "height", self.height.max(1));
        set_f64(pad, "alpha", self.alpha.clamp(0.0, 1.0));
    }
}

/// How an apply leaves the pads it wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Write {
    /// Straight to where the scene wants them. What a take has always been.
    Cut,
    /// Left where they were, for a caller that is going to ease them over.
    Hold,
    /// At the target, but invisible, for a scene arriving beside the one on
    /// air while a transition brings it up.
    Enter,
}

/// One slot on its way from where it was to where a scene wants it.
#[derive(Debug, Clone)]
pub struct Ramp {
    pub pad: gst::Pad,
    pub from: PadState,
    pub to: PadState,
}

/// One slot: a fixed chain and the compositor pad at the end of it.
pub struct Slot {
    pub index: usize,
    /// Closed when this slot is hidden and is not a source's home slot, so a
    /// slot nobody is looking at costs one dropped buffer per frame.
    gate: gst::Element,
    pub queue: gst::Element,
    crop: gst::Element,
    flip: gst::Element,
    pub pad: gst::Pad,
    /// Everything in the chain, for a teardown.
    elements: Vec<gst::Element>,
    /// The source whose tee this slot is fed from, and the tee pad it took.
    bound: Option<Bound>,
    /// Held with its last frame while a source is rebuilt. Not available for
    /// binding, not cleared by an apply.
    retired: bool,
    /// The one slot a source keeps whatever the scene says, so its picture is
    /// always at the compositor and a take of it never waits for a frame.
    home: bool,
    /// The scene item this slot drew last. A re-apply prefers the slot that
    /// already has the item, so a transition that put the incoming scene on
    /// fresh pads does not shuffle it back onto the old ones afterwards and
    /// flash.
    drawn: Option<Id>,
    /// The per item filters on this slot's chain, in order, between the flip
    /// and the compositor pad. Rebuilt only when the item asks for a different
    /// chain; an apply that changes nothing takes no pad block.
    filters: Vec<FilterSlot>,
    /// What those filters were built from, for the comparison that decides
    /// whether to touch the graph at all.
    filter_shape: Vec<ItemFilter>,
    /// How the last placement drawn here trims and fits its picture, and the
    /// item's own motions, for a transition that has to reach them.
    look: Look,
}

/// The parts of a placement a transition needs after the apply has gone.
#[derive(Debug, Clone, Copy, Default)]
struct Look {
    crop: (f64, f64, f64, f64),
    sizing: Sizing,
    align: (f64, f64),
    rotation: f64,
    motion: Motion,
}

/// What a slot's chain is fed from.
enum Bound {
    /// A source's programme tee, which is every ordinary item.
    Source {
        source: SourceId,
        tee_pad: gst::Pad,
        /// The source's list of pads drawing it, which this slot's pad joined
        /// at bind and must leave at unbind. Left in it, the slot reused for
        /// another source was still the old source's: the overlay board drew
        /// the old text into the new item's box, on air, and the old source's
        /// timeline shift could land on the new one's pad.
        pads: std::sync::Arc<crate::plugin::branch::VideoPads>,
    },
    /// A group composited on its own, because a filter over it cannot be
    /// expressed by flattening. See `mixer::group`.
    Group { item: Id, sub: Box<super::group::SubCompositor> },
}

impl Bound {
    fn source(&self) -> Option<&SourceId> {
        match self {
            Bound::Source { source, .. } => Some(source),
            Bound::Group { .. } => None,
        }
    }
}

impl Slot {
    /// True when this slot can be handed to a placement.
    fn free(&self) -> bool {
        !self.retired
    }

    /// True when this slot already carries that source's picture, so binding
    /// it costs nothing.
    fn holds(&self, source: &SourceId) -> bool {
        self.bound.as_ref().and_then(Bound::source).is_some_and(|s| s == source)
    }

    /// True when this slot is already compositing that group.
    fn holds_group(&self, item: Option<Id>) -> bool {
        matches!((&self.bound, item), (Some(Bound::Group { item: have, .. }), Some(want)) if *have == want)
    }

    pub fn source(&self) -> Option<&SourceId> {
        self.bound.as_ref().and_then(Bound::source)
    }

    /// The sources a group slot is compositing, or the one source an ordinary
    /// slot is bound to.
    pub fn sources(&self) -> Vec<SourceId> {
        match &self.bound {
            Some(Bound::Group { sub, .. }) => sub.sources(),
            Some(Bound::Source { source, .. }) => vec![source.clone()],
            None => Vec::new(),
        }
    }

    /// Everything a placement decides, written straight onto the pad and the
    /// two elements above it. Nothing here allocates and nothing blocks.
    fn draw(&self, p: &Placement, z: u32, write: Write) -> Ramp {
        let from = PadState::read(&self.pad);
        let to = PadState::of(p);
        set_u32(&self.pad, "zorder", z);
        // `Hold` keeps the pad where it is and hands the move to the ramp. The
        // z order, the crop, the flip and the sizing policy are steps whatever
        // happens: there is no halfway between two crops worth drawing.
        match write {
            Write::Cut => to.write(&self.pad),
            Write::Hold => from.write(&self.pad),
            Write::Enter => PadState { alpha: 0.0, ..to }.write(&self.pad),
        }
        set_sizing(&self.pad, p.sizing);
        for (name, v) in [("xalign", p.align.0), ("yalign", p.align.1)] {
            if self.pad.has_property(name) {
                set_f64(&self.pad, name, v.clamp(0.0, 1.0));
            }
        }
        self.set_crop(p);
        self.set_rotation(p.rotation);
        Ramp { pad: self.pad.clone(), from, to }
    }

    fn remember(&mut self, p: &Placement) {
        self.drawn = p.item;
        self.look = Look {
            crop: p.crop,
            sizing: p.sizing,
            align: p.align,
            rotation: p.rotation,
            motion: p.motion,
        };
    }

    /// This slot as a transition sees it: going from `from` to `to`.
    pub(crate) fn leg(&self, source: SourceId, from: PadState, to: PadState) -> Leg {
        Leg {
            pad: self.pad.clone(),
            item: self.drawn,
            source,
            from,
            to,
            crop: self.crop_target(&to),
            motion: self.look.motion,
        }
    }

    /// What a wipe needs to trim this slot's picture: the crop element, the
    /// item's own crop in pixels, and where the picture lands on the canvas.
    ///
    /// `None` for a picture that is turned (the crop is before the flip, so
    /// its axes are not the canvas's) and for a slot that has had no frame
    /// yet, which has no size to trim against.
    fn crop_target(&self, at: &PadState) -> Option<CropTarget> {
        let quarters = (self.look.rotation.rem_euclid(360.0) / 90.0).round() as i64 % 4;
        if quarters != 0 {
            return None;
        }
        let sink = self.crop.static_pad("sink")?;
        let (fw, fh) = sink.current_caps().and_then(|c| frame_size(&c))?;
        let (fw, fh) = (fw as f64, fh as f64);
        let c = self.look.crop;
        let base = [
            (c.0.clamp(0.0, 0.95) * fw).round(),
            (c.1.clamp(0.0, 0.95) * fh).round(),
            (c.2.clamp(0.0, 0.95) * fw).round(),
            (c.3.clamp(0.0, 0.95) * fh).round(),
        ];
        let source = ((fw - base[0] - base[2]).max(2.0), (fh - base[1] - base[3]).max(2.0));
        let boxed = Rect::of(at);
        let picture = match self.look.sizing {
            Sizing::Contain => {
                let s = (boxed.w / source.0).min(boxed.h / source.1);
                let (w, h) = (source.0 * s, source.1 * s);
                let (ax, ay) = self.look.align;
                Rect::new(boxed.x + (boxed.w - w) * ax, boxed.y + (boxed.h - h) * ay, w, h)
            }
            _ => boxed,
        };
        if picture.w < 1.0 || picture.h < 1.0 {
            return None;
        }
        Some(CropTarget {
            element: self.crop.clone(),
            base,
            picture,
            scale: (source.0 / picture.w, source.1 / picture.h),
            source,
            offset_ns: stream_offset(&sink),
        })
    }

    /// The pad's state right now, for a transition that has to ramp from it.
    pub fn state(&self) -> PadState {
        PadState::read(&self.pad)
    }

    pub fn pad(&self) -> &gst::Pad {
        &self.pad
    }

    /// The item this slot is drawing, when a document named one.
    pub fn item(&self) -> Option<Id> {
        self.drawn
    }

    /// True when this slot is on the canvas: a pad at alpha 0 is skipped
    /// before conversion, so this is what the compositor is paying for.
    pub fn showing(&self) -> bool {
        self.pad.property::<f64>("alpha") > 0.0
    }

    /// True when pictures are still reaching this slot's compositor pad,
    /// whether or not the pad is drawn.
    ///
    /// Not the same question as [`Slot::showing`], and the difference is a
    /// second of programme output: see [`Slot::hide`].
    pub fn feeding(&self) -> bool {
        !self.gate.property::<bool>("drop")
    }

    /// Hide this slot without unbinding it, which is what makes the next take
    /// a property write.
    ///
    /// A slot with a source on it keeps its picture flowing, whether it is
    /// drawn or not. The compositor skips a pad at alpha 0 before any
    /// conversion, so a hidden pad costs almost nothing to blend, and having
    /// the frame already there is what makes a take show the current picture
    /// rather than a stale one.
    ///
    /// Shutting the valve instead looks like the saving and is not. A
    /// compositor sink pad that has had buffers and then stops getting them is
    /// not an inactive pad: `ignore-inactive-pads` covers a pad that has never
    /// produced anything, and this one has. So the aggregator waits for it,
    /// and what it waits is the whole of `MIN_UPSTREAM_LATENCY_NS`, one
    /// second. Measured on this Mac on 2026-09-17, with a take every five
    /// seconds between a two box and a single: the programme compositor
    /// produced the frame for running time 11.900 and then nothing at all for
    /// 1.061 s, once for every take that took a slot off air, which is the
    /// whole of the 50.6 ms the sixty frame stall gauge read against a 34 ms
    /// bar. A slot with nothing bound to it is a different case: nothing feeds
    /// it, its pad has never seen a buffer, the aggregator ignores it, and its
    /// valve shuts so the chain is not woken for a picture nobody wants.
    fn hide(&self) {
        set_f64(&self.pad, "alpha", 0.0);
        if !self.home && self.bound.is_none() {
            self.gate.set_property("drop", true);
        }
    }

    /// Let the picture through again.
    fn open(&self) {
        if self.gate.property::<bool>("drop") {
            self.gate.set_property("drop", false);
        }
    }

    /// The crop, in whole source pixels, from the normalised fractions the
    /// document stores.
    ///
    /// The source's own size is the canvas: every input is normalised to the
    /// canvas contract before it reaches the programme pipeline, which is what
    /// makes one number here right for every kind of source.
    fn set_crop(&self, p: &Placement) {
        let caps = self
            .gate
            .static_pad("sink")
            .and_then(|pad| pad.current_caps())
            .and_then(|c| frame_size(&c));
        let (w, h) = caps.unwrap_or((0, 0));
        if w == 0 || h == 0 {
            // Nothing has flowed yet, so there is no picture to trim. The next
            // apply, or the next visibility tick, writes it.
            return;
        }
        let px = |f: f64, of: i32| (f.clamp(0.0, 0.95) * of as f64).round() as i32;
        for (name, value, of) in [
            ("left", p.crop.0, w),
            ("top", p.crop.1, h),
            ("right", p.crop.2, w),
            ("bottom", p.crop.3, h),
        ] {
            set_i32_on(&self.crop, name, px(value, of));
        }
    }

    /// Rotation, snapped to the nearest right angle.
    ///
    /// `videoflip` turns by quarters and nothing else. Arbitrary rotation
    /// needs `gltransformation` and the frame on the GPU, which is the
    /// graphics catalogue's job and not this one; until then an item asking
    /// for 37 degrees gets 45 rounded to 0 and the reference page says so.
    fn set_rotation(&self, degrees: f64) {
        let quarters = (degrees.rem_euclid(360.0) / 90.0).round() as i64 % 4;
        let method = match quarters {
            1 => "clockwise",
            2 => "rotate-180",
            3 => "counterclockwise",
            _ => "none",
        };
        self.flip.set_property_from_str("method", method);
    }
}

/// The compositor, its slots, and the arithmetic that decides which slot a
/// placement lands in.
pub struct SlotPool {
    program: gst::Pipeline,
    vmix: gst::Element,
    slots: Vec<Slot>,
    canvas: CanvasCaps,
    /// Bumped every time a slot has to be relinked, so the cost of a scene is
    /// visible in the log and in a test rather than guessed at.
    misses: u64,
    /// Slots held for the scene going out during a transition. They are not
    /// available for binding and are not hidden by an apply, because both
    /// scenes are on the canvas until the transition ends.
    crossing: Vec<usize>,
    /// The programme's overlay board, which draws a keyed item's picture
    /// after the compositor. None in a pool built without one, and its
    /// keys are then flattened over black. See `filters::chroma`.
    board: Option<std::sync::Arc<crate::overlay::Board>>,
}

impl SlotPool {
    /// Build the pool into a pipeline that is not running yet.
    pub fn build(
        program: &gst::Pipeline,
        vmix: &gst::Element,
        canvas: &CanvasCaps,
    ) -> Result<SlotPool> {
        let mut pool = SlotPool {
            program: program.clone(),
            vmix: vmix.clone(),
            slots: Vec::new(),
            canvas: canvas.clone(),
            misses: 0,
            crossing: Vec::new(),
            board: None,
        };
        for _ in 0..INITIAL_SLOTS {
            pool.grow()?;
        }
        Ok(pool)
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Draw keyed items on this board rather than flattening them.
    pub fn use_board(&mut self, board: std::sync::Arc<crate::overlay::Board>) {
        self.board = Some(board);
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// How many times applying a scene has had to relink a slot. Zero for a
    /// show that never places a source in more spots than it has slots. The
    /// binding a source gets when it is added is not one of these: it is the
    /// relink that buys every later take its freedom.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Add one slot. Called at build time, and on demand when a scene wants
    /// more places than the pool has.
    fn grow(&mut self) -> Result<&mut Slot> {
        let index = self.slots.len();
        // `valve` resends the sticky events when it opens again, so a slot
        // coming back gets its caps and segment without anything reconnecting.
        let gate = make("valve", &format!("slot-gate-{index}"))?;
        gate.set_property("drop", true);
        let queue = gstutil::queue_thread(&format!("slot-q-{index}"))?;
        let crop = make("videocrop", &format!("slot-crop-{index}"))?;
        let flip = make("videoflip", &format!("slot-flip-{index}"))?;
        let elements = vec![gate.clone(), queue.clone(), crop.clone(), flip.clone()];
        self.program.add_many(&elements).context("adding a compositor slot")?;
        gst::Element::link_many(elements.iter().collect::<Vec<_>>())
            .context("linking a compositor slot")?;

        let pad = self
            .vmix
            .request_pad_simple("sink_%u")
            .context("the compositor refused another slot pad")?;
        answer_allocation(&self.vmix, &pad);
        // A slot arrives invisible and full canvas: nothing reaches programme
        // until a scene asks for it.
        set_u32(&pad, "zorder", Z_LIVE + index as u32);
        set_f64(&pad, "alpha", 0.0);
        set_i32(&pad, "xpos", 0);
        set_i32(&pad, "ypos", 0);
        if pad.has_property("sizing-policy") {
            pad.set_property_from_str("sizing-policy", "keep-aspect-ratio");
        }
        flip.static_pad("src")
            .context("a slot's flip has no src pad")?
            .link(&pad)
            .context("linking a slot into the compositor")?;
        for el in &elements {
            el.sync_state_with_parent().ok();
        }

        self.slots.push(Slot {
            index,
            gate,
            queue,
            crop,
            flip,
            pad,
            elements,
            bound: None,
            retired: false,
            home: false,
            drawn: None,
            filters: Vec::new(),
            filter_shape: Vec::new(),
            look: Look::default(),
        });
        Ok(self.slots.last_mut().expect("just pushed"))
    }

    /// Give a source one slot as soon as it is added, so that taking it later
    /// is property writes and nothing else.
    ///
    /// This is what makes `program.take {source}` cost the same as it did
    /// before the pool existed. The relink happens while the source has not
    /// produced a frame yet, where it costs nothing; without it the first take
    /// of every source would be a cache miss.
    pub fn reserve(&mut self, branch: &ProgrammeBranch) -> Result<()> {
        if self.slots.iter().any(|s| s.free() && s.holds(&branch.id)) {
            return Ok(());
        }
        let index = self.free_slot(&branch.id, &[])?;
        // Not counted as a miss: this is the relink that buys every later take
        // its freedom, and it happens while the source has no picture yet.
        self.bind(index, branch, false)?;
        // The home slot: open whatever the scene says, so this source's
        // picture is always at the compositor and a take of it is instant.
        self.slots[index].home = true;
        self.slots[index].open();
        Ok(())
    }

    /// Apply a scene: bind, write, hide the rest.
    ///
    /// `branches` is every source the mixer has, by id, so a placement can be
    /// linked to the one it names. A placement of a source that is not there
    /// is skipped and reported: an agent gets the names it could have used
    /// from the caller, not a black frame with no explanation.
    pub fn apply<'a>(
        &mut self,
        placements: &[Placement],
        branches: &[(&'a SourceId, &'a ProgrammeBranch)],
    ) -> Result<Applied> {
        self.apply_move(placements, branches, Write::Cut)
    }

    /// The same, with every slot left where it was so a caller can ease it
    /// over. `Applied::ramps` is what it has to write.
    pub fn apply_ramped<'a>(
        &mut self,
        placements: &[Placement],
        branches: &[(&'a SourceId, &'a ProgrammeBranch)],
    ) -> Result<Applied> {
        self.apply_move(placements, branches, Write::Hold)
    }

    /// Put a scene on the canvas beside the one that is already there.
    ///
    /// Slot pressure doubles for the length of a transition, because both
    /// scenes are drawn at once: the outgoing slots are held out of the pool
    /// and the incoming scene is bound to different ones, growing the pool if
    /// it has to. Every incoming pad arrives at alpha 0 and stays there until
    /// a curve raises it, so the frame between binding and the first sync is
    /// never the wrong picture.
    ///
    /// The caller must finish with [`SlotPool::end_transition`] whatever
    /// happens, or the outgoing scene never leaves.
    pub fn begin_transition<'a>(
        &mut self,
        placements: &[Placement],
        branches: &[(&'a SourceId, &'a ProgrammeBranch)],
    ) -> Result<Crossed> {
        let out: Vec<Leg> = self
            .slots
            .iter()
            .filter(|s| !s.retired && s.showing())
            .map(|s| {
                let from = s.state();
                s.leg(s.source().cloned().unwrap_or_default(), from, PadState { alpha: 0.0, ..from })
            })
            .collect();
        self.crossing = self
            .slots
            .iter()
            .filter(|s| !s.retired && s.showing())
            .map(|s| s.index)
            .collect();
        let applied = match self.apply_move(placements, branches, Write::Enter) {
            Ok(applied) => applied,
            Err(e) => {
                // A crossing that could not be bound leaves the scene on air
                // exactly as it was. The held slots go back first, or nothing
                // would ever hide them again.
                self.crossing.clear();
                return Err(e);
            }
        };
        let incoming = applied
            .slots
            .iter()
            .zip(placements.iter().filter(|p| branches.iter().any(|(id, _)| *id == &p.source)))
            .map(|(index, p)| {
                let to = PadState::of(p);
                self.slots[*index].leg(p.source.clone(), PadState { alpha: 0.0, ..to }, to)
            })
            .collect();
        Ok(Crossed { out, incoming, applied })
    }

    /// Let go of the slots the outgoing scene was held on.
    ///
    /// The next apply hides whatever nobody claimed, which is the same
    /// declarative path a take has always taken, so there is no separate
    /// teardown to get wrong.
    pub fn end_transition(&mut self) {
        self.crossing.clear();
    }

    /// True while both scenes are on the canvas.
    pub fn crossing(&self) -> bool {
        !self.crossing.is_empty()
    }

    /// Put the outgoing scene under or over the incoming one for the length
    /// of a transition. The outgoing slots are held, so no apply touches their
    /// z order until the transition ends and the next apply claims them.
    pub fn layer(&self, out: &[Leg], layering: Layering) {
        let band = match layering {
            Layering::AsIs => return,
            Layering::IncomingOver => Z_UNDER,
            Layering::OutgoingOver => Z_OVER,
        };
        for leg in out {
            let z: u32 = leg.pad.property("zorder");
            let within = z.saturating_sub(Z_LIVE).min(Z_LIVE - Z_UNDER - 1);
            leg.pad.set_property("zorder", band + within);
        }
    }

    /// Hold the slots drawing these items out of the next apply, so an item
    /// hidden on air stays on the canvas while it plays its way off. Answers
    /// a leg per item found, going from where it is to nowhere.
    pub fn hold_items(&mut self, items: &[Id]) -> Vec<Leg> {
        let mut legs = Vec::new();
        for slot in &self.slots {
            let Some(item) = slot.drawn else { continue };
            if slot.retired || !slot.showing() || !items.contains(&item) {
                continue;
            }
            let from = slot.state();
            legs.push(slot.leg(slot.source().cloned().unwrap_or_default(), from, PadState { alpha: 0.0, ..from }));
            self.crossing.push(slot.index);
        }
        legs
    }

    /// The slots that took these placements in the last apply, as legs
    /// arriving where each placement says.
    pub fn legs_of(&self, placements: &[&Placement]) -> Vec<Leg> {
        placements
            .iter()
            .filter_map(|p| {
                let slot = self.slots.iter().find(|s| {
                    p.item.is_some() && s.drawn == p.item && !self.crossing.contains(&s.index)
                })?;
                let to = PadState::of(p);
                Some(slot.leg(p.source.clone(), PadState { alpha: slot.state().alpha, ..to }, to))
            })
            .collect()
    }

    fn apply_move<'a>(
        &mut self,
        placements: &[Placement],
        branches: &[(&'a SourceId, &'a ProgrammeBranch)],
        write: Write,
    ) -> Result<Applied> {
        let mut claimed: Vec<usize> = Vec::with_capacity(placements.len());
        let mut ramps: Vec<Ramp> = Vec::with_capacity(placements.len());
        let mut missing: Vec<SourceId> = Vec::new();
        let mut drawn = 0usize;

        for (i, p) in placements.iter().enumerate() {
            // A group carrying a filter is composited on its own, so it is
            // bound to a sub compositor rather than to one source's tee. See
            // `mixer::group`: the expensive path, taken only when a scene asks
            // for it.
            if !p.group.is_empty() {
                let index = match self.pick_group(p.item, &claimed) {
                    Some(index) => index,
                    None => self.free_slot(&p.source, &claimed)?,
                };
                if let Err(e) = self.bind_group(index, p, branches) {
                    warn!(?e, "a filtered group could not be composited; it was left out");
                    continue;
                }
                self.slots[index].open();
                if let Err(e) = self.sync_filters(index, &p.filters) {
                    warn!(slot = index, ?e, "a group's filter could not go on");
                }
                ramps.push(self.slots[index].draw(p, Z_LIVE + i as u32, write));
                self.slots[index].remember(p);
                claimed.push(index);
                drawn += 1;
                continue;
            }
            let Some((_, branch)) = branches.iter().find(|(id, _)| *id == &p.source) else {
                if !missing.contains(&p.source) {
                    missing.push(p.source.clone());
                }
                continue;
            };
            let index = match self.pick(&p.source, p.item, &claimed) {
                Some(index) => index,
                None => {
                    let index = self.free_slot(&p.source, &claimed)?;
                    self.bind(index, branch, true)?;
                    index
                }
            };
            self.slots[index].open();
            if let Err(e) = self.sync_filters(index, &p.filters) {
                // The picture is still on air with whatever chain it had. A
                // filter that will not build is a refused change, not a black
                // frame, and the message names the item's own filter.
                warn!(slot = index, source = %p.source, ?e, "a scene item's filter could not go on");
                // Once, not twice a second. The shape is remembered as it was
                // asked for, so the next apply finds nothing new to build and
                // leaves the pad alone until the item's filters change.
                self.slots[index].filter_shape = p.filters.to_vec();
            }
            ramps.push(self.slots[index].draw(p, Z_LIVE + i as u32, write));
            self.slots[index].remember(p);
            claimed.push(index);
            drawn += 1;
        }

        let held = self.crossing.clone();
        for slot in &self.slots {
            if !slot.retired && !claimed.contains(&slot.index) && !held.contains(&slot.index) {
                slot.hide();
            }
        }
        // A binding to a source is kept whatever the scene says, because it is
        // free and because keeping it is what makes the next take a property
        // write. A sub compositor is neither: it is a compositor, a queue and
        // a chain per child, so a group nobody is drawing gives them all back.
        let stale: Vec<usize> = self
            .slots
            .iter()
            .filter(|s| !s.retired && !claimed.contains(&s.index) && !held.contains(&s.index))
            .filter(|s| matches!(s.bound, Some(Bound::Group { .. })))
            .map(|s| s.index)
            .collect();
        for index in stale {
            self.unbind(index);
        }
        Ok(Applied { drawn, missing, slots: claimed, ramps })
    }

    /// A slot that already holds this source and has not been claimed yet.
    ///
    /// The slot that drew this very item wins, when the document named one.
    /// Without that rule a re-apply after a transition would find the source
    /// on the pad the outgoing scene used and move the picture back onto it,
    /// which is a frame of the wrong thing for no reason.
    fn pick(&self, source: &SourceId, item: Option<Id>, claimed: &[usize]) -> Option<usize> {
        let free = |s: &&Slot| self.usable(s.index) && !claimed.contains(&s.index);
        if item.is_some() {
            let same = self
                .slots
                .iter()
                .find(|s| free(s) && s.holds(source) && s.drawn == item)
                .map(|s| s.index);
            if same.is_some() {
                return same;
            }
        }
        self.slots.iter().find(|s| free(s) && s.holds(source)).map(|s| s.index)
    }

    /// The slot already compositing this group, so a group whose children only
    /// moved costs property writes and nothing else.
    fn pick_group(&self, item: Option<Id>, claimed: &[usize]) -> Option<usize> {
        self.slots
            .iter()
            .find(|s| self.usable(s.index) && !claimed.contains(&s.index) && s.holds_group(item))
            .map(|s| s.index)
    }

    /// True when a slot can be handed to a placement: not held with a freeze
    /// frame, and not drawing the scene a transition is taking off the canvas.
    fn usable(&self, index: usize) -> bool {
        self.slots.get(index).is_some_and(|s| s.free()) && !self.crossing.contains(&index)
    }

    /// A slot to relink, preferring one bound to nothing, then one whose
    /// source is not in this scene, then a new one.
    fn free_slot(&mut self, source: &SourceId, claimed: &[usize]) -> Result<usize> {
        let empty = self
            .slots
            .iter()
            .find(|s| self.usable(s.index) && s.bound.is_none() && !claimed.contains(&s.index))
            .map(|s| s.index);
        if let Some(index) = empty {
            return Ok(index);
        }
        let reusable = self
            .slots
            .iter()
            .find(|s| self.usable(s.index) && !s.holds(source) && !claimed.contains(&s.index))
            .map(|s| s.index);
        if let Some(index) = reusable {
            return Ok(index);
        }
        info!(slots = self.slots.len(), "the scene wants more places than the pool has; growing it");
        Ok(self.grow()?.index)
    }

    /// Put this item's filters on this slot's chain and take off what it no
    /// longer wants.
    ///
    /// The comparison comes first and is the whole point: a scene reapplied
    /// twice a second by the visibility tick must not take a pad block twice a
    /// second. Only a chain that is actually different is built.
    fn sync_filters(&mut self, index: usize, want: &[ItemFilter]) -> Result<()> {
        let same = self.slots[index].filter_shape.len() == want.len()
            && self
                .slots[index]
                .filter_shape
                .iter()
                .zip(want)
                .all(|(have, want)| have.shape() == want.shape());
        if same {
            return Ok(());
        }
        if self.configure_filters(index, want) {
            return Ok(());
        }
        self.clear_filters(index);
        let live = self.program.current_state() == gst::State::Playing;
        let canvas = self.canvas.clone();
        for (n, f) in want.iter().enumerate() {
            let upstream = match self.slots[index].filters.last() {
                Some(last) => last.bin.clone(),
                None => self.slots[index].flip.clone(),
            };
            let spec = FilterSpec {
                id: format!("slot-{index}-filter-{n}"),
                type_id: f.type_id.clone(),
                side: FilterSide::SceneItem,
                params: f.params.clone(),
            };
            let board = self.board.clone();
            let built = crate::plugin::filter::make(&f.type_id).and_then(|filter| {
                let pad = self.slots[index].pad.clone();
                // Turned on before the first frame goes through, so a key
                // never shows a frame flattened over black on its way in.
                let hook = filter.board();
                super::keyed::attach(board.as_ref(), hook.clone(), &pad, &board_key(index, n));
                let at = Insertion::before_pad(&self.program, &upstream, &pad);
                let placed = crate::plugin::filter::insert(at, spec, filter, &canvas, live);
                if placed.is_err() {
                    super::keyed::detach(board.as_ref(), hook, &board_key(index, n));
                }
                placed
            });
            match built {
                Ok(slot) => self.slots[index].filters.push(slot),
                Err(e) => {
                    // Half a chain is worse than none: what is there comes off
                    // again so the picture is the clean one it was before.
                    self.clear_filters(index);
                    return Err(e);
                }
            }
        }
        self.slots[index].filter_shape = want.to_vec();
        Ok(())
    }

    /// Change the filters on a slot in place, when the item asks for the
    /// same filters in the same order with different settings. True when
    /// every one of them took the change; false leaves the chain to be built
    /// again, and a filter that took its change already is rebuilt with it.
    ///
    /// A slider dragged in the inspector is this path: the key's settings
    /// change on the running filter, and nothing is relinked.
    fn configure_filters(&mut self, index: usize, want: &[ItemFilter]) -> bool {
        let slot = &mut self.slots[index];
        let same_kinds = slot.filters.len() == want.len()
            && slot.filter_shape.len() == want.len()
            && slot.filter_shape.iter().zip(want).all(|(have, want)| have.type_id == want.type_id);
        if !same_kinds {
            return false;
        }
        for (filter, want) in slot.filters.iter_mut().zip(want) {
            if filter.spec.params == want.params {
                continue;
            }
            match filter.configure(&want.params) {
                Ok(crate::plugin::Configure::Applied) => {}
                _ => return false,
            }
        }
        slot.filter_shape = want.to_vec();
        true
    }

    /// Take every filter off a slot's chain, newest first.
    ///
    /// Newest first because each insert put itself between the one below it
    /// and the pad, so only the top one's recorded neighbours are still the
    /// ones it actually sits between.
    fn clear_filters(&mut self, index: usize) {
        let filters = std::mem::take(&mut self.slots[index].filters);
        self.slots[index].filter_shape.clear();
        for (n, f) in filters.iter().enumerate() {
            super::keyed::detach(self.board.as_ref(), f.board(), &board_key(index, n));
        }
        for f in filters.into_iter().rev() {
            let id = f.spec.id.clone();
            if let Err(e) = f.remove() {
                warn!(filter = %id, ?e, "could not take a scene item's filter off its slot");
            }
        }
    }

    /// Link a slot to a source's tee. The one place in the whole apply path
    /// that touches the graph, and the reason `reserve` exists.
    fn bind(&mut self, index: usize, branch: &ProgrammeBranch, miss: bool) -> Result<()> {
        self.unbind(index);
        self.clear_eos(index);
        let slot = &mut self.slots[index];
        let sink = slot.gate.static_pad("sink").context("a slot's valve has no sink pad")?;
        let tee_pad = branch
            .vtee
            .request_pad_simple("src_%u")
            .with_context(|| format!("the tee of {} refused a pad", branch.id))?;

        // Hidden before it is linked, so a slot being wired up is never a
        // frame of the wrong picture.
        slot.hide();
        // The pad offset before anything flows: a pad offset adjusts the
        // segment as it traverses the pad, so a pad given one after buffers
        // are flowing keeps the timeline it already decided on.
        branch.pads.attach(&slot.pad);

        let linked = if self.program.current_state() == gst::State::Playing {
            // A cache miss on a running programme. The block is on this
            // source's own queue, not on the programme's path: the compositor
            // keeps aggregating its other pads and `force-live` keeps the
            // output on schedule, so the cost is this source's own frames for
            // as long as the block holds and nothing downstream notices.
            if miss {
                self.misses += 1;
            }
            let src = branch.vq.static_pad("src").context("the video queue has no src pad")?;
            let (tee_pad, sink) = (tee_pad.clone(), sink.clone());
            gstutil::with_pad_blocked(&src, BLOCK_TIMEOUT, move || {
                if let Err(e) = tee_pad.link(&sink) {
                    warn!(?e, "could not link a slot to a source tee");
                }
            })
            .context("binding a slot to a source while the programme is running")
        } else {
            tee_pad.link(&sink).map(|_| ()).map_err(anyhow::Error::from)
        };
        if let Err(e) = linked {
            branch.pads.detach(&self.slots[index].pad);
            branch.vtee.release_request_pad(&tee_pad);
            return Err(e);
        }
        for el in &self.slots[index].elements {
            el.sync_state_with_parent().ok();
        }
        self.slots[index].bound =
            Some(Bound::Source { source: branch.id.clone(), tee_pad, pads: branch.pads.clone() });
        debug!(slot = index, source = %branch.id, "slot bound");
        Ok(())
    }

    /// Take back the end of stream `unbind` left on an empty slot.
    ///
    /// `unbind` ends an empty slot's input so the live compositor stops
    /// waiting on it, and counted on the next source's stream start to clear
    /// that. It does not when the stream start is one the pad has already
    /// seen, a source coming back to a slot it had before: the pad kept its
    /// EOS, the compositor treated the input as finished, and the source was
    /// in the scene, on its slot, at full opacity, and not drawn. Seen on air
    /// with a test source. A flush always clears it, and is safe here: the
    /// slot is hidden and holds no frame, and it waits for one compositor
    /// frame first as `unbind` does.
    fn clear_eos(&mut self, index: usize) {
        let slot = &self.slots[index];
        if !slot.pad.pad_flags().contains(gst::PadFlags::EOS) {
            return;
        }
        let Some(chain) = slot.queue.static_pad("sink") else { return };
        set_f64(&slot.pad, "alpha", 0.0);
        crate::slow_step!("slot frame barrier", index, gstutil::after_next_frame(&slot.pad));
        gstutil::wake_chain(&chain);
        gstutil::resume_chain(&chain);
        debug!(slot = index, "cleared the end of stream an empty slot was left with");
    }

    /// Bind a slot to a group composited on its own.
    ///
    /// The one path that adds elements to a running programme on purpose, and
    /// the reason it is worth it: a filter over a group cannot be expressed by
    /// flattening, so either the group gets its own compositor or a blur over
    /// three items is three blurs. Built only for a group that carries a
    /// filter, and taken down the moment it does not. See `mixer::group`.
    fn bind_group<'a>(
        &mut self,
        index: usize,
        p: &Placement,
        branches: &[(&'a SourceId, &'a ProgrammeBranch)],
    ) -> Result<()> {
        let item = p.item.context("a group placement with no item id cannot be bound")?;
        if !self.slots[index].holds_group(p.item) {
            self.unbind(index);
            let mut sub = super::group::SubCompositor::build(
                &self.program,
                &self.canvas,
                &format!("{index}"),
            )?;
            let sink = self.slots[index]
                .gate
                .static_pad("sink")
                .context("a slot's valve has no sink pad")?;
            let src = sub
                .output()
                .static_pad("src")
                .context("a sub compositor has no src pad")?;
            self.slots[index].hide();
            if let Err(e) = src.link(&sink) {
                sub.teardown();
                return Err(e.into());
            }
            for el in &self.slots[index].elements {
                el.sync_state_with_parent().ok();
            }
            self.slots[index].bound = Some(Bound::Group { item, sub: Box::new(sub) });
            self.misses += 1;
            info!(slot = index, "a group carrying a filter is being composited on its own");
        }
        let Some(Bound::Group { sub, .. }) = self.slots[index].bound.as_mut() else {
            anyhow::bail!("the slot did not take the group")
        };
        sub.apply(&p.group, branches)
    }

    /// Take a slot off whatever it was showing. The tee pad goes back so a
    /// source that is removed does not leave one behind.
    fn unbind(&mut self, index: usize) {
        if self.slots[index].bound.is_none() {
            // Nothing feeds this chain, so nothing is parked in it.
            crate::slow_step!("slot clear_filters", index, self.clear_filters(index));
            return;
        }
        // The chain is flushed before anything on it is touched, and put back
        // at the end.
        //
        // A slot is a live path into the compositor. Its queue can be holding
        // a buffer whose running time the compositor has not reached, and the
        // thread that pushed it waits on that pad until it does; the queue
        // behind it holds every serialized query that arrived after that
        // buffer, and those queries reach back up the source's tee, across the
        // proxy and into the source's own pipeline. Unlinking the tee pad does
        // not wake any of that: the parked thread is below the link being cut.
        // So the source's branch could not then be taken to NULL, because a
        // pad only deactivates once its streaming thread is out of it, and the
        // mixer command loop sat in `source.remove` for a minute at a time
        // waiting for a query to come back through a chain it was itself
        // dismantling. Measured on this Mac on 2026-09-17: 2.8 s, 9.1 s,
        // 27.0 s and 58.1 s inside one two minute soak.
        //
        // The flush wakes every one of them and drops the frames of the source
        // that is leaving, which is what the next source on this slot wants
        // anyway: a slot must never open with the last one's picture.
        //
        // Flushed at the queue rather than at the valve above it. A valve that
        // is dropping swallows every serialized event, and `FLUSH_STOP` is
        // serialized while `FLUSH_START` is not, so a flush sent in above a
        // valve this function is about to close goes in and never comes out:
        // the slot stays flushing, the next bind cannot send its sticky events
        // down it, and the programme loses the picture for good.
        //
        // Out of the picture before any of it, and one frame let out. A pad
        // at alpha 0 is one the compositor does not convert, and a frame
        // pushed since proves nothing older is in flight, which is what makes
        // the flush stop at the end of this safe to send: see
        // `gstutil::after_next_frame`. The wait is here and not beside the
        // flush stop because a compositor cycle run on a pad that is between
        // the two is its own trouble: a whole frame of that window aborted
        // this crate's tests in `gst_video_aggregator_fill_queues`.
        set_f64(&self.slots[index].pad, "alpha", 0.0);
        crate::slow_step!("slot frame barrier", index, gstutil::after_next_frame(&self.slots[index].pad));
        let chain = self.slots[index].queue.static_pad("sink");
        if let Some(pad) = &chain {
            crate::slow_step!("slot flush", index, gstutil::wake_chain(pad));
        }
        // A filter chain belongs to the item that asked for it, not to the
        // slot, so a slot going to a different source loses it here rather
        // than carrying a chroma key onto the next camera.
        crate::slow_step!("slot clear_filters", index, self.clear_filters(index));
        let Some(bound) = self.slots[index].bound.take() else {
            if let Some(pad) = &chain {
                gstutil::resume_chain(pad);
            }
            return;
        };
        let slot = &mut self.slots[index];
        slot.home = false;
        slot.drawn = None;
        slot.hide();
        let sink = slot.gate.static_pad("sink");
        match bound {
            Bound::Source { tee_pad, pads, .. } => {
                pads.detach(&self.slots[index].pad);
                if let Some(sink) = sink {
                    crate::slow_step!("slot unlink from the source tee", index, {
                        let _ = tee_pad.unlink(&sink);
                    });
                }
                if let Some(tee) = tee_pad.parent_element() {
                    crate::slow_step!(
                        "slot release of the source tee pad",
                        index,
                        tee.release_request_pad(&tee_pad)
                    );
                }
            }
            Bound::Group { mut sub, .. } => {
                if let (Some(sink), Some(src)) = (sink, sub.output().static_pad("src")) {
                    let _ = src.unlink(&sink);
                }
                crate::slow_step!("slot group teardown", index, sub.teardown());
            }
        }
        // Back in service. The running time is kept, because every other slot
        // is still on air against it.
        if let Some(pad) = &chain {
            crate::slow_step!("slot flush stop", index, gstutil::resume_chain(pad));
            // An empty slot has no next buffer to wait for. End this input
            // after its flush so the live compositor can ignore it immediately.
            // The next binding sends stream-start, which clears the pad's EOS.
            self.slots[index].pad.send_event(gst::event::Eos::new());
        }
    }

    /// Hold every slot showing this source with its last frame, under the live
    /// band and over the slate, because its pipeline is being rebuilt.
    ///
    /// The compositor keeps drawing a pad's last buffer for as long as the pad
    /// is there, so this is a freeze frame that costs nothing: no element to
    /// add, no picture to copy, no code path that only runs during a fault.
    /// Returns the slots it held, for `release`.
    pub fn retire(&mut self, source: &SourceId) -> Vec<usize> {
        let mut held = Vec::new();
        for slot in &mut self.slots {
            if !slot.holds(source) || slot.retired {
                continue;
            }
            let showing = slot.pad.property::<f64>("alpha") > 0.0;
            let z = (Z_RETIRED + held.len() as u32).min(Z_RETIRED_TOP);
            set_u32(&slot.pad, "zorder", z);
            if !showing {
                // A slot that was not on air has nothing worth freezing, and
                // sixteen frozen pads at alpha 1 is sixteen pads the
                // compositor has to blend.
                set_f64(&slot.pad, "alpha", 0.0);
            }
            slot.retired = true;
            held.push(slot.index);
        }
        held
    }

    /// Let go of held slots: unbind them and put them back in the pool.
    pub fn release(&mut self, held: &[usize]) {
        let n = self.slots.len();
        for index in held.iter().copied().filter(|i| *i < n) {
            self.unbind(index);
            self.slots[index].retired = false;
            set_u32(&self.slots[index].pad, "zorder", Z_LIVE + index as u32);
        }
    }

    /// Unbind every slot showing this source, for a source that is being
    /// removed rather than rebuilt.
    pub fn drop_source(&mut self, source: &SourceId) {
        let indexes: Vec<usize> =
            self.slots.iter().filter(|s| s.holds(source)).map(|s| s.index).collect();
        for index in indexes {
            self.unbind(index);
            self.slots[index].retired = false;
            set_u32(&self.slots[index].pad, "zorder", Z_LIVE + index as u32);
        }
    }

    /// What is on one slot's chain right now, by filter id. For the status,
    /// for `gmx dot`, and for the test that proves a per item filter is in the
    /// pipeline rather than only in the document.
    pub fn filters_on(&self, index: usize) -> Vec<String> {
        self.slots
            .get(index)
            .map(|s| s.filters.iter().map(|f| f.spec.type_id.clone()).collect())
            .unwrap_or_default()
    }

    /// Every per item filter in the pool, as `(slot, type)`.
    pub fn item_filters(&self) -> Vec<(usize, String)> {
        self.slots
            .iter()
            .flat_map(|s| s.filters.iter().map(move |f| (s.index, f.spec.type_id.clone())))
            .collect()
    }

    /// The sources whose pads a transition is driving on this property right
    /// now. What a test reads to prove a curve reached the pad rather than
    /// being dropped on the way.
    /// The slots whose crop a transition is driving right now.
    pub fn driven_crops(&self) -> Vec<usize> {
        self.slots
            .iter()
            .filter(|s| ["left", "top", "right", "bottom"].iter().any(|p| driven(&s.crop, p)))
            .map(|s| s.index)
            .collect()
    }

    pub fn driven_by_a_transition(&self, property: &str) -> Vec<SourceId> {
        self.slots
            .iter()
            .filter(|s| driven(&s.pad, property))
            .map(|s| s.source().cloned().unwrap_or_default())
            .collect()
    }

    /// The compositor itself, for a probe that has to read its output.
    pub fn compositor(&self) -> &gst::Element {
        &self.vmix
    }

    /// Every slot, for the mixer's status and for a test.
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// The slots a source is drawn in right now.
    pub fn slots_of(&self, source: &SourceId) -> Vec<usize> {
        self.slots
            .iter()
            .filter(|s| s.holds(source) || s.sources().iter().any(|id| id == source))
            .map(|s| s.index)
            .collect()
    }

    /// Every group being composited on its own, as `(slot, children)`. What
    /// the status and `gmx dot` read, and what a test counts.
    pub fn sub_compositors(&self) -> Vec<(usize, Vec<SourceId>)> {
        self.slots
            .iter()
            .filter(|s| matches!(s.bound, Some(Bound::Group { .. })))
            .map(|s| (s.index, s.sources()))
            .collect()
    }

    /// How many slots are visible. What the compositor is actually paying for.
    pub fn visible(&self) -> usize {
        self.slots.iter().filter(|s| s.pad.property::<f64>("alpha") > 0.0).count()
    }

    /// Take every slot out of the pipeline. Only at shutdown.
    pub fn teardown(&mut self) {
        let indexes: Vec<usize> = self.slots.iter().map(|s| s.index).collect();
        for index in indexes {
            self.unbind(index);
        }
        for slot in &self.slots {
            // The compositor's pad first, which flushes it and frees a slot
            // thread parked in its chain function; a queue taken to NULL
            // before that joins a thread that never returns. Then each
            // element locked, so the programme's own state walk cannot put it
            // back up before it is removed.
            self.vmix.release_request_pad(&slot.pad);
            for el in &slot.elements {
                el.set_locked_state(true);
                let _ = el.set_state(gst::State::Null);
                let _ = self.program.remove(el);
            }
        }
        self.slots.clear();
    }
}

/// Both scenes on the canvas: what a transition drives.
///
/// Plain pads and pad states. What happens to them is [`crate::mixer::transition`]'s
/// decision, not this module's, which is why a wipe written in Python lands on
/// the same frame a built in fade does.
pub struct Crossed {
    /// Pads drawing the scene that is going away.
    pub out: Vec<Leg>,
    /// Pads drawing the scene arriving, at alpha 0 until a curve raises them.
    pub incoming: Vec<Leg>,
    /// What the apply itself did, for the log and the missing source list.
    pub applied: Applied,
}

/// What one apply did, for the log, the status and the tests.
#[derive(Debug, Clone, Default)]
pub struct Applied {
    /// Placements that reached a slot.
    pub drawn: usize,
    /// Sources the scene named that this mixer does not have.
    pub missing: Vec<SourceId>,
    /// The slots that were claimed, in scene order.
    pub slots: Vec<usize>,
    /// Where each claimed slot was and where the scene wants it. Empty on the
    /// ordinary path, where the pad is already where it is going.
    pub ramps: Vec<Ramp>,
}

/// Everything a placement decides, onto a pad that is not a slot's.
///
/// The sub compositor's children want exactly what a slot wants, and the
/// arithmetic is the same arithmetic, so it lives here once rather than in two
/// places that could drift.
pub(crate) fn write_pad(
    pad: &gst::Pad,
    p: &Placement,
    canvas: &CanvasCaps,
    crop: &gst::Element,
    flip: &gst::Element,
) {
    PadState::of(p).write(pad);
    set_sizing(pad, p.sizing);
    for (name, v) in [("xalign", p.align.0), ("yalign", p.align.1)] {
        if pad.has_property(name) {
            set_f64(pad, name, v.clamp(0.0, 1.0));
        }
    }
    // The child's own size is the canvas, because every input is normalised to
    // the canvas contract before it reaches the programme pipeline.
    let px = |f: f64, of: i32| (f.clamp(0.0, 0.95) * of as f64).round() as i32;
    for (name, value, of) in [
        ("left", p.crop.0, canvas.width),
        ("top", p.crop.1, canvas.height),
        ("right", p.crop.2, canvas.width),
        ("bottom", p.crop.3, canvas.height),
    ] {
        set_i32_on(crop, name, px(value, of));
    }
    let quarters = (p.rotation.rem_euclid(360.0) / 90.0).round() as i64 % 4;
    flip.set_property_from_str(
        "method",
        match quarters {
            1 => "clockwise",
            2 => "rotate-180",
            3 => "counterclockwise",
            _ => "none",
        },
    );
}

/// Write `sizing-policy`, if this pad has it and has a value it understands.
///
/// `set_property_from_str` panics inside glib on a nick the enum does not
/// carry, so the nick is looked up first: a `compositor` built before
/// `keep-aspect-ratio-with-crop` existed gets the next best thing rather than
/// taking the mixer thread down mid take.
///
/// The value the pad already has is read before anything is written, and this
/// one is worth a comment. `compositor` marks its pad's video converter dirty
/// whenever `sizing-policy` is set, whatever it is set to, and rebuilds the
/// converter on the next aggregation. The visibility tick reapplies the whole
/// scene twice a second, so an unconditional write here threw away and rebuilt
/// a converter per pad twice a second and put the programme's frame interval
/// over 34 ms on an idle mixer. Comparing first is the difference between a
/// late frame every half second and none.
pub(crate) fn set_sizing(pad: &gst::Pad, sizing: Sizing) {
    // A pad whose box a transition is trimming has been told to fill it,
    // because the box is already the shape of the trimmed picture. Putting the
    // item's own policy back halfway through would letterbox it.
    if driven(pad, "width") {
        return;
    }
    let Some(pspec) = pad.find_property("sizing-policy") else { return };
    let class = glib::EnumClass::with_type(pspec.value_type());
    let held = pad.property_value("sizing-policy");
    let now = glib::EnumValue::from_value(&held).map(|(_, v)| v.nick().to_string());
    for nick in sizing.nicks() {
        let known = class.as_ref().and_then(|c| c.value_by_nick(nick)).is_some();
        if known {
            if now.as_deref() != Some(*nick) {
                pad.set_property_from_str("sizing-policy", nick);
            }
            return;
        }
    }
    warn!(
        pad = %pad.name(),
        ?sizing,
        "this compositor has none of the sizing policies this item could use"
    );
}

/// Read a frame size off caps, for turning a normalised crop into pixels.
fn frame_size(caps: &gst::Caps) -> Option<(i32, i32)> {
    let s = caps.structure(0)?;
    Some((s.get("width").ok()?, s.get("height").ok()?))
}

/// Whether a transition owns this property of this pad right now.
///
/// A control binding is a function of the frame's running time, so a property
/// written by hand under one is undone on the next sync anyway. What it also
/// does is put one wrong frame on air, and the supervisor's visibility tick
/// runs twice a second whatever else is happening. So every write here asks
/// first, and the tick stops stamping on a live transition without knowing
/// that transitions exist.
///
/// What it asks is whether a binding is live. A pad keeps its bindings for as
/// long as it is on the compositor, because taking one off a running
/// aggregator is a use after free (`mixer::transition::Controllers` says why),
/// so a pad that has ever been in a transition holds one for ever and that one
/// is disabled between transitions.
fn driven(pad: &impl IsA<gst::Object>, name: &str) -> bool {
    pad.control_binding(name).is_some_and(|b| !b.is_disabled())
}

/// Answer the allocation query at a slot's compositor pad, on the software
/// compositor.
///
/// A slot whose picture changes size (a crop changed on air, and a wipe that
/// changes it every frame) renegotiates, and the flip in front of the pad
/// then asks the compositor how to allocate. The compositor answers a
/// serialized query only once the buffer queued ahead of it has been blended,
/// and the slot's thread waits for that. Measured on the first wipe: the
/// flip waited 480 ms on one query, the slot got five buffers through in half
/// a second and the compositor drew stale, wrongly trimmed frames; and the
/// query that went out when the crop settled back never came back at all, so
/// that slot drew nothing again until the mixer stopped. `compositor` has no
/// pool to offer a sink pad anyway, so answering here (video meta, no pool,
/// which is what the element in front needs to allocate for itself) changes
/// nothing about where buffers come from and lets the caps go down in order
/// with the buffers. A probe that sees only queries costs nothing per frame.
///
/// Not on a GPU compositor, whose answer carries the context its uploads need.
fn answer_allocation(vmix: &gst::Element, pad: &gst::Pad) {
    let software = vmix.factory().is_some_and(|f| f.name() == "compositor");
    if !software {
        return;
    }
    pad.add_probe(gst::PadProbeType::QUERY_DOWNSTREAM, |_pad, info| {
        let Some(gst::PadProbeData::Query(query)) = &mut info.data else {
            return gst::PadProbeReturn::Ok;
        };
        let gst::QueryViewMut::Allocation(allocation) = query.view_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        allocation.add_allocation_meta::<gstreamer_video::VideoMeta>(None);
        gst::PadProbeReturn::Handled
    });
}

/// What to add to a running time to get the stream time an element upstream
/// of the compositor syncs its bindings at: the segment's `time` less its
/// `base`. Zero for a pad that has had no segment, which is a pad with no
/// picture to trim.
fn stream_offset(pad: &gst::Pad) -> i64 {
    let Some(event) = pad.sticky_event::<gst::event::Segment>(0) else { return 0 };
    let Some(seg) = event.segment().downcast_ref::<gst::ClockTime>() else { return 0 };
    let ns = |t: Option<gst::ClockTime>| t.map(|t| t.nseconds() as i64).unwrap_or(0);
    ns(seg.time()) - ns(seg.base())
}

/// Property writes that do not fight the element over its own type.
///
/// A compositor pad's `width` is an int, its `alpha` a double and its `zorder`
/// a uint, and setting one with the wrong Rust type panics inside glib rather
/// than failing. Writing them through these three is how the wrong one becomes
/// a compile error.
fn set_i32(pad: &gst::Pad, name: &str, v: i32) {
    if pad.property::<i32>(name) != v && !driven(pad, name) {
        pad.set_property(name, v);
    }
}

fn set_u32(pad: &gst::Pad, name: &str, v: u32) {
    if pad.property::<u32>(name) != v && !driven(pad, name) {
        pad.set_property(name, v);
    }
}

fn set_f64(pad: &gst::Pad, name: &str, v: f64) {
    if (pad.property::<f64>(name) - v).abs() > f64::EPSILON && !driven(pad, name) {
        pad.set_property(name, v);
    }
}

/// The same three, for an element rather than a pad.
fn set_i32_on(el: &gst::Element, name: &str, v: i32) {
    if el.property::<i32>(name) != v && !driven(el, name) {
        el.set_property(name, v);
    }
}
