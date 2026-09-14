//! The slot pool: how a scene reaches the compositor without adding an
//! element.
//!
//! One compositor, a fixed pool of slots, and the scene tree flattened before
//! it gets here. A slot is a chain that never changes shape:
//!
//! ```text
//!   pgm-vtee-{source} =|=> slot-q-N -> slot-crop-N -> slot-flip-N -> vmix:sink_N
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
//! z is banded so the slate and the freeze frame keep working:
//!
//! | Band | What is in it |
//! |---|---|
//! | 0 | the slate, permanently, fully opaque |
//! | 1 to 99 | the branches of sources being rebuilt, holding their last frame |
//! | 1000 and up | the live items of the scene, bottom of the stack first |

use crate::caps::CanvasCaps;
use crate::gstutil::{self, make};
use crate::plugin::branch::ProgrammeBranch;
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

/// How long to wait for a slot's tee pad to reach an idle point on a cache
/// miss. The figure the filters and the proxy swaps already use.
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

/// What a scene wants drawn in one slot.
///
/// Plain numbers in canvas pixels, worked out from the document by
/// `scene::geometry::flatten` before anything here is called. Nothing in this
/// module knows what a scene document is.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub source: SourceId,
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
    /// `keep-aspect-ratio`, `scale` or `none`, as the compositor spells it.
    pub sizing: &'static str,
    /// 0 to 1 each, where the picture sits inside its frame when the sizing
    /// policy leaves room.
    pub align: (f64, f64),
    pub audio: PlacementAudio,
}

impl Placement {
    /// A source filling the whole canvas, which is what a bare source id means.
    pub fn full_canvas(source: SourceId, canvas: &CanvasCaps) -> Placement {
        Placement {
            source,
            xpos: 0,
            ypos: 0,
            width: canvas.width,
            height: canvas.height,
            alpha: 1.0,
            crop: (0.0, 0.0, 0.0, 0.0),
            rotation: 0.0,
            sizing: "keep-aspect-ratio",
            align: (0.5, 0.5),
            audio: PlacementAudio::Follow,
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

/// One slot: a fixed chain and the compositor pad at the end of it.
pub struct Slot {
    pub index: usize,
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
}

struct Bound {
    source: SourceId,
    tee_pad: gst::Pad,
}

impl Slot {
    /// True when this slot can be handed to a placement.
    fn free(&self) -> bool {
        !self.retired
    }

    /// True when this slot already carries that source's picture, so binding
    /// it costs nothing.
    fn holds(&self, source: &SourceId) -> bool {
        self.bound.as_ref().is_some_and(|b| &b.source == source)
    }

    pub fn source(&self) -> Option<&SourceId> {
        self.bound.as_ref().map(|b| &b.source)
    }

    /// Everything a placement decides, written straight onto the pad and the
    /// two elements above it. Nothing here allocates and nothing blocks.
    fn draw(&self, p: &Placement, z: u32) {
        set_u32(&self.pad, "zorder", z);
        set_f64(&self.pad, "alpha", p.alpha.clamp(0.0, 1.0));
        set_i32(&self.pad, "xpos", p.xpos);
        set_i32(&self.pad, "ypos", p.ypos);
        set_i32(&self.pad, "width", p.width.max(0));
        set_i32(&self.pad, "height", p.height.max(0));
        // `sizing-policy` is what `fit` maps onto. A compositor that does not
        // have it scales to the box, which is `stretch`, and the reference
        // page says so rather than the picture quietly being wrong.
        if self.pad.has_property("sizing-policy") {
            self.pad.set_property_from_str("sizing-policy", p.sizing);
        }
        for (name, v) in [("xalign", p.align.0), ("yalign", p.align.1)] {
            if self.pad.has_property(name) {
                set_f64(&self.pad, name, v.clamp(0.0, 1.0));
            }
        }
        self.set_crop(p);
        self.set_rotation(p.rotation);
    }

    /// Hide this slot without unbinding it. The picture is still flowing into
    /// the compositor, which is what makes the next take a property write, and
    /// a pad at alpha 0 is skipped before any conversion happens.
    fn hide(&self) {
        set_f64(&self.pad, "alpha", 0.0);
    }

    /// The crop, in whole source pixels, from the normalised fractions the
    /// document stores.
    ///
    /// The source's own size is the canvas: every input is normalised to the
    /// canvas contract before it reaches the programme pipeline, which is what
    /// makes one number here right for every kind of source.
    fn set_crop(&self, p: &Placement) {
        let caps = self
            .queue
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
    /// Bumped every time a slot has to be relinked, so the cost of a scene is
    /// visible in the log and in a test rather than guessed at.
    misses: u64,
}

impl SlotPool {
    /// Build the pool into a pipeline that is not running yet.
    pub fn build(program: &gst::Pipeline, vmix: &gst::Element) -> Result<SlotPool> {
        let mut pool =
            SlotPool { program: program.clone(), vmix: vmix.clone(), slots: Vec::new(), misses: 0 };
        for _ in 0..INITIAL_SLOTS {
            pool.grow()?;
        }
        Ok(pool)
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// How many times a slot has had to be relinked. Zero for a show that
    /// never places a source in more spots than it has slots.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Add one slot. Called at build time, and on demand when a scene wants
    /// more places than the pool has.
    fn grow(&mut self) -> Result<&mut Slot> {
        let index = self.slots.len();
        let queue = gstutil::queue_thread(&format!("slot-q-{index}"))?;
        let crop = make("videocrop", &format!("slot-crop-{index}"))?;
        let flip = make("videoflip", &format!("slot-flip-{index}"))?;
        let elements = vec![queue.clone(), crop.clone(), flip.clone()];
        self.program.add_many(&elements).context("adding a compositor slot")?;
        gst::Element::link_many(elements.iter().collect::<Vec<_>>())
            .context("linking a compositor slot")?;

        let pad = self
            .vmix
            .request_pad_simple("sink_%u")
            .context("the compositor refused another slot pad")?;
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
            queue,
            crop,
            flip,
            pad,
            elements,
            bound: None,
            retired: false,
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
        self.bind(index, branch)?;
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
        let mut claimed: Vec<usize> = Vec::with_capacity(placements.len());
        let mut missing: Vec<SourceId> = Vec::new();
        let mut drawn = 0usize;

        for (i, p) in placements.iter().enumerate() {
            let Some((_, branch)) = branches.iter().find(|(id, _)| *id == &p.source) else {
                if !missing.contains(&p.source) {
                    missing.push(p.source.clone());
                }
                continue;
            };
            let index = match self.pick(&p.source, &claimed) {
                Some(index) => index,
                None => {
                    let index = self.free_slot(&p.source, &claimed)?;
                    self.bind(index, branch)?;
                    index
                }
            };
            self.slots[index].draw(p, Z_LIVE + i as u32);
            claimed.push(index);
            drawn += 1;
        }

        for slot in &self.slots {
            if !slot.retired && !claimed.contains(&slot.index) {
                slot.hide();
            }
        }
        Ok(Applied { drawn, missing, slots: claimed })
    }

    /// A slot that already holds this source and has not been claimed yet.
    fn pick(&self, source: &SourceId, claimed: &[usize]) -> Option<usize> {
        self.slots
            .iter()
            .find(|s| s.free() && s.holds(source) && !claimed.contains(&s.index))
            .map(|s| s.index)
    }

    /// A slot to relink, preferring one bound to nothing, then one whose
    /// source is not in this scene, then a new one.
    fn free_slot(&mut self, source: &SourceId, claimed: &[usize]) -> Result<usize> {
        let empty = self
            .slots
            .iter()
            .find(|s| s.free() && s.bound.is_none() && !claimed.contains(&s.index))
            .map(|s| s.index);
        if let Some(index) = empty {
            return Ok(index);
        }
        let reusable = self
            .slots
            .iter()
            .find(|s| s.free() && !s.holds(source) && !claimed.contains(&s.index))
            .map(|s| s.index);
        if let Some(index) = reusable {
            return Ok(index);
        }
        info!(slots = self.slots.len(), "the scene wants more places than the pool has; growing it");
        Ok(self.grow()?.index)
    }

    /// Link a slot to a source's tee. The one place in the whole apply path
    /// that touches the graph, and the reason `reserve` exists.
    fn bind(&mut self, index: usize, branch: &ProgrammeBranch) -> Result<()> {
        self.unbind(index);
        let slot = &mut self.slots[index];
        let sink = slot.queue.static_pad("sink").context("a slot's queue has no sink pad")?;
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
            self.misses += 1;
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
        self.slots[index].bound = Some(Bound { source: branch.id.clone(), tee_pad });
        debug!(slot = index, source = %branch.id, "slot bound");
        Ok(())
    }

    /// Take a slot off whatever it was showing. The tee pad goes back so a
    /// source that is removed does not leave one behind.
    fn unbind(&mut self, index: usize) {
        let Some(bound) = self.slots[index].bound.take() else { return };
        let slot = &self.slots[index];
        slot.hide();
        if let Some(sink) = slot.queue.static_pad("sink") {
            let _ = bound.tee_pad.unlink(&sink);
        }
        if let Some(tee) = bound.tee_pad.parent_element() {
            tee.release_request_pad(&bound.tee_pad);
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

    /// Every slot, for the mixer's status and for a test.
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// The slots a source is drawn in right now.
    pub fn slots_of(&self, source: &SourceId) -> Vec<usize> {
        self.slots.iter().filter(|s| s.holds(source)).map(|s| s.index).collect()
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
            for el in &slot.elements {
                let _ = el.set_state(gst::State::Null);
                let _ = self.program.remove(el);
            }
            self.vmix.release_request_pad(&slot.pad);
        }
        self.slots.clear();
    }
}

/// What one apply did, for the log, the status and the tests.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    /// Placements that reached a slot.
    pub drawn: usize,
    /// Sources the scene named that this mixer does not have.
    pub missing: Vec<SourceId>,
    /// The slots that were claimed, in scene order.
    pub slots: Vec<usize>,
}

/// Read a frame size off caps, for turning a normalised crop into pixels.
fn frame_size(caps: &gst::Caps) -> Option<(i32, i32)> {
    let s = caps.structure(0)?;
    Some((s.get("width").ok()?, s.get("height").ok()?))
}

/// Property writes that do not fight the element over its own type.
///
/// A compositor pad's `width` is an int, its `alpha` a double and its `zorder`
/// a uint, and setting one with the wrong Rust type panics inside glib rather
/// than failing. Writing them through these three is how the wrong one becomes
/// a compile error.
fn set_i32(pad: &gst::Pad, name: &str, v: i32) {
    if pad.property::<i32>(name) != v {
        pad.set_property(name, v);
    }
}

fn set_u32(pad: &gst::Pad, name: &str, v: u32) {
    if pad.property::<u32>(name) != v {
        pad.set_property(name, v);
    }
}

fn set_f64(pad: &gst::Pad, name: &str, v: f64) {
    if (pad.property::<f64>(name) - v).abs() > f64::EPSILON {
        pad.set_property(name, v);
    }
}

/// The same three, for an element rather than a pad.
fn set_i32_on(el: &gst::Element, name: &str, v: i32) {
    if el.property::<i32>(name) != v {
        el.set_property(name, v);
    }
}
