//! Transitions: one scene becoming another on the compositor's own clock.
//!
//! A take is a property write and costs nothing. A transition is the same
//! property, written many times, and the question is who writes it. A thread
//! that wakes sixty times a second and sets `alpha` is easy and is what the
//! geometry commands used; what it cannot do is land on a frame. The thread
//! sleeps, the scheduler wakes it when it feels like it, and the value the
//! compositor reads is whatever happened to be there when it blended.
//!
//! So a transition here is a set of curves rather than a set of writes. Each
//! curve is a `GstInterpolationControlSource` bound to one property of one
//! compositor pad, holding a handful of timed values in **running time**. The
//! aggregator calls `gst_object_sync_values` on every pad once per output
//! frame with the running time of the frame it is about to blend, so the value
//! that lands on the picture is the value the curve says for that picture. A
//! frame late or early is not possible: the number is a function of the frame.
//!
//! ```text
//!   take ---> curves ---> control sources ---> compositor pad properties
//!               |                                   ^
//!               |  bound at the running time        |  sampled once per
//!               |  the take was armed for           |  output frame
//!               +-----------------------------------+
//! ```
//!
//! # What a transition needs that a cut does not
//!
//! Both scenes on the canvas at once. The outgoing scene keeps its slots for
//! the length of the transition and the incoming scene is bound to different
//! ones, so slot pressure doubles while it runs and the pool grows if it has
//! to. `SlotPool::begin_transition` is what reserves them.
//!
//! # The fallback
//!
//! A property that will not take a control binding (an older element, a
//! compositor on the GPU whose pad does not declare `controllable`) falls back
//! to the property thread in `mixer.rs`. That is the only thing the thread is
//! for now. `Bound::unbound` is what it is handed.
//!
//! # Who owns a pad's property
//!
//! One binding per pad and property, made the first time a transition needs it
//! and then left on the pad. A transition rewrites the curve behind it and
//! turns it on; settling turns it off. Nothing is ever taken off a pad while
//! the compositor is running, because the aggregator walks that list every
//! frame without a lock and without a reference. [`Controllers`] has the
//! detail and the `gstobject.c` line it rests on.
//!
//! # Who decides the shape
//!
//! [`Transition`] does. `cut`, `fade`, `move` and `stinger` are in here; a
//! `transition` plugin over the sidecar host is sampled once per frame of the
//! transition before it starts and its answers become the same curves, so a
//! plugin written in any language lands on the frame exactly as a built in
//! does. See `docs/reference/transitions.md`.

use crate::mixer::slots::PadState;
use crate::scene::id::Id;
use crate::state::SourceId;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_controller::prelude::*;
use gstreamer_controller::{InterpolationControlSource, InterpolationMode};
use std::time::Duration;
use tracing::{debug, warn};

/// How many points a curve is sampled at, per second of transition.
///
/// The aggregator interpolates linearly between the points it is given, so the
/// only thing this decides is how closely the eased shape is followed. Sixty a
/// second is finer than any easing curve needs and is still a few dozen
/// `GstControlPoint`s for a transition nobody runs longer than two seconds.
const SAMPLES_PER_SECOND: u64 = 60;

/// The longest transition this build will run.
///
/// Both scenes are on the canvas for the whole of it, so a transition that
/// never ends is a scene that never leaves. Ten seconds is longer than any
/// stinger and short enough that a typo cannot park the mixer.
pub const MAX_DURATION_MS: u64 = 10_000;

/// The properties a transition may drive, in the order a reader expects them.
///
/// Named here rather than spelled at each call site because writing a property
/// a pad does not have panics inside glib, and because the reference page is
/// generated from this list.
pub const DRIVEN: &[&str] = &["alpha", "xpos", "ypos", "width", "height"];

// ---------------------------------------------------------------------------
// What a caller asks for
// ---------------------------------------------------------------------------

/// Which transition, and how long it takes.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionSpec {
    pub kind: Kind,
    pub duration_ms: u64,
}

impl Default for TransitionSpec {
    fn default() -> Self {
        TransitionSpec { kind: Kind::Cut, duration_ms: 0 }
    }
}

impl TransitionSpec {
    /// A cut: no curves, no window, nothing to wait for.
    pub fn cut() -> Self {
        TransitionSpec::default()
    }

    /// True when this is a cut in everything but name. A zero duration is a
    /// cut whatever the type says, which is what makes `duration_ms: 0` the
    /// documented way to ask for one.
    pub fn is_cut(&self) -> bool {
        self.kind == Kind::Cut || self.duration_ms == 0
    }

    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration_ms.min(MAX_DURATION_MS))
    }
}

impl From<&godwinmix_protocol::requests::Transition> for TransitionSpec {
    /// What a client asked for, as the mixer has it.
    ///
    /// A name this build does not know becomes `Kind::Plugin`, which the mixer
    /// hands to the transition renderer; the control layer has already refused
    /// a name that no plugin answers to, so nothing gets here by accident.
    fn from(request: &godwinmix_protocol::requests::Transition) -> Self {
        let kind = match request.type_id().as_str() {
            "cut" => Kind::Cut,
            "fade" => Kind::Fade,
            "move" => Kind::Move,
            "stinger" => Kind::Stinger {
                clip: request.param_str("clip").unwrap_or_default(),
                cut_at_ms: request.param_u64("cut_at_ms"),
                luma: request.param_bool("luma").unwrap_or(true),
            },
            other => Kind::Plugin(other.to_string()),
        };
        TransitionSpec { kind, duration_ms: request.duration_ms() }
    }
}

/// The transitions this build runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// The next frame is the new scene. What a take has always been.
    Cut,
    /// Both scenes on the canvas, one fading out under the other.
    Fade,
    /// Items that appear in both scenes travel from where they were to where
    /// they are going; everything else fades.
    Move,
    /// A clip drawn over both scenes, with the cut underneath it at the point
    /// where the clip covers the canvas.
    Stinger {
        /// The clip, as a source id already in the mixer or a file URI.
        clip: String,
        /// When the scenes swap underneath, in milliseconds from the start.
        /// Half way by default, which is where a stinger's cover usually is.
        cut_at_ms: Option<u64>,
        /// Key the clip's black out, so its luma is its coverage. The canvas
        /// stays I420: only the clip's own pad carries alpha.
        luma: bool,
    },
    /// A `transition` plugin over the sidecar host, by plugin name.
    Plugin(String),
}

impl Kind {
    /// The name on the wire.
    pub fn name(&self) -> &str {
        match self {
            Kind::Cut => "cut",
            Kind::Fade => "fade",
            Kind::Move => "move",
            Kind::Stinger { .. } => "stinger",
            Kind::Plugin(name) => name,
        }
    }

    /// The built in names, for an error that lists what a caller could have
    /// asked for.
    pub const BUILT_IN: &'static [&'static str] = &["cut", "fade", "move", "stinger"];
}

// ---------------------------------------------------------------------------
// What a transition is given
// ---------------------------------------------------------------------------

/// One pad's part in a transition.
#[derive(Debug, Clone)]
pub struct Leg {
    pub pad: gst::Pad,
    /// The scene item this pad is drawing, when the document named one. What
    /// `move` matches on.
    pub item: Option<Id>,
    pub source: SourceId,
    /// Where the pad is now.
    pub from: PadState,
    /// Where it should be when the transition ends.
    pub to: PadState,
}

impl Leg {
    /// A stable name for this pad, for a plugin's `render` and for a log.
    pub fn id(&self) -> String {
        self.pad.name().to_string()
    }
}

/// Everything a transition is told about the crossing.
#[derive(Debug, Clone)]
pub struct Crossing {
    /// Pads drawing the scene on air, which are going away.
    pub out: Vec<Leg>,
    /// Pads drawing the scene coming on, hidden at `from`.
    pub incoming: Vec<Leg>,
    /// Audiomixer pads, with where each one is and where it is going.
    pub audio: Vec<(gst::Pad, f64, f64)>,
    /// The clip drawn over both scenes, for a stinger.
    pub cover: Option<Leg>,
    /// Running time the transition starts at, on the compositor's own
    /// timeline, which is what the aggregator samples against.
    pub start: gst::ClockTime,
    pub duration: gst::ClockTime,
}

impl Crossing {
    pub fn end(&self) -> gst::ClockTime {
        self.start + self.duration
    }

    /// Outgoing and incoming pads matched by item id, then by source.
    ///
    /// By item id first because that is what the document means by "the same
    /// thing in both scenes": an item that is the inset in one scene and full
    /// screen in the next is one item and should travel. By source second so
    /// two scenes built independently, where nobody thought about ids, still
    /// move the camera that is in both of them rather than dissolving it into
    /// itself.
    pub fn matched(&self) -> Vec<(usize, usize)> {
        let mut pairs = Vec::new();
        let mut used: Vec<usize> = Vec::new();
        for (i, out) in self.out.iter().enumerate() {
            let by_item = out.item.and_then(|id| {
                self.incoming
                    .iter()
                    .position(|to| to.item == Some(id) && !used.contains(&to_index(to, self)))
            });
            let found = by_item.or_else(|| {
                self.incoming
                    .iter()
                    .position(|to| to.source == out.source && !used.contains(&to_index(to, self)))
            });
            if let Some(j) = found {
                if !used.contains(&j) {
                    used.push(j);
                    pairs.push((i, j));
                }
            }
        }
        pairs
    }
}

/// Index of a leg within the incoming list. A helper rather than a closure so
/// `matched` reads as the two rules it is.
fn to_index(leg: &Leg, x: &Crossing) -> usize {
    x.incoming.iter().position(|l| l.pad == leg.pad).unwrap_or(usize::MAX)
}

// ---------------------------------------------------------------------------
// What a transition answers with
// ---------------------------------------------------------------------------

/// One property of one pad, over the window of the transition.
#[derive(Debug, Clone)]
pub struct Curve {
    pub pad: gst::Pad,
    pub property: &'static str,
    /// Timed values in running time, in order.
    pub points: Vec<(gst::ClockTime, f64)>,
}

impl Curve {
    /// The value the property should be left at when the binding is taken off.
    fn settle(&self) -> f64 {
        self.points.last().map(|(_, v)| *v).unwrap_or(0.0)
    }
}

/// A transition decides the curves and nothing else.
///
/// It never touches a pad, never starts a thread and never waits: given where
/// every pad is and where it is going, it says what the numbers should be at
/// every point of the window. That is what makes a transition testable without
/// a pipeline, and what lets a plugin in another language be one.
pub trait Transition: Send + Sync {
    fn name(&self) -> &str;
    fn curves(&self, x: &Crossing) -> Vec<Curve>;
}

/// Build the transition a spec names.
///
/// A plugin transition is not built here: the mixer samples the plugin and
/// hands the answers to [`from_samples`], because only the mixer can reach the
/// plugin host.
pub fn built_in(kind: &Kind) -> Option<Box<dyn Transition>> {
    match kind {
        Kind::Cut => Some(Box::new(Cut)),
        Kind::Fade => Some(Box::new(Fade)),
        Kind::Move => Some(Box::new(Move)),
        Kind::Stinger { cut_at_ms, .. } => Some(Box::new(Stinger { cut_at_ms: *cut_at_ms })),
        Kind::Plugin(_) => None,
    }
}

// ---------------------------------------------------------------------------
// The built in four
// ---------------------------------------------------------------------------

/// The next frame is the new scene.
pub struct Cut;

impl Transition for Cut {
    fn name(&self) -> &str {
        "cut"
    }

    fn curves(&self, _x: &Crossing) -> Vec<Curve> {
        Vec::new()
    }
}

/// Both scenes on the canvas, one fading out under the other.
///
/// The incoming pads rise from nothing to the alpha their scene asks for and
/// the outgoing pads fall to nothing. Geometry is not touched: an item that is
/// in both scenes in different places is two pictures crossing, which is what
/// a dissolve looks like and is why `move` exists for when it is not what you
/// wanted.
pub struct Fade;

impl Transition for Fade {
    fn name(&self) -> &str {
        "fade"
    }

    fn curves(&self, x: &Crossing) -> Vec<Curve> {
        let mut curves = Vec::new();
        for leg in &x.out {
            curves.push(ramp(&leg.pad, "alpha", x, leg.from.alpha, 0.0));
        }
        for leg in &x.incoming {
            curves.push(ramp(&leg.pad, "alpha", x, 0.0, leg.to.alpha));
        }
        curves
    }
}

/// Items in both scenes travel; everything else fades.
pub struct Move;

impl Transition for Move {
    fn name(&self) -> &str {
        "move"
    }

    fn curves(&self, x: &Crossing) -> Vec<Curve> {
        let pairs = x.matched();
        let mut curves = Vec::new();
        for (i, leg) in x.out.iter().enumerate() {
            if pairs.iter().any(|(a, _)| *a == i) {
                // Its picture continues on the incoming pad, so this one goes
                // straight away rather than lingering under it at half alpha.
                curves.push(step(&leg.pad, "alpha", x, leg.from.alpha, 0.0, 0.0));
            } else {
                curves.push(ramp(&leg.pad, "alpha", x, leg.from.alpha, 0.0));
            }
        }
        for (j, leg) in x.incoming.iter().enumerate() {
            match pairs.iter().find(|(_, b)| *b == j) {
                Some((i, _)) => {
                    let was = x.out[*i].from;
                    curves.extend(travel(&leg.pad, x, &was, &leg.to));
                }
                None => curves.push(ramp(&leg.pad, "alpha", x, 0.0, leg.to.alpha)),
            }
        }
        curves
    }
}

/// A clip over both scenes, with the cut underneath it.
///
/// The scenes do not dissolve: they swap on one frame, at the point the clip
/// covers the canvas, which is what makes a stinger look like an edit rather
/// than a mix. The clip itself rises and falls around that point.
pub struct Stinger {
    pub cut_at_ms: Option<u64>,
}

impl Transition for Stinger {
    fn name(&self) -> &str {
        "stinger"
    }

    fn curves(&self, x: &Crossing) -> Vec<Curve> {
        let total = x.duration.nseconds().max(1);
        let cut_ns = match self.cut_at_ms {
            Some(ms) => gst::ClockTime::from_mseconds(ms).nseconds().min(total),
            None => total / 2,
        };
        let at = cut_ns as f64 / total as f64;
        let mut curves = Vec::new();
        for leg in &x.out {
            curves.push(step(&leg.pad, "alpha", x, leg.from.alpha, 0.0, at));
        }
        for leg in &x.incoming {
            curves.push(step(&leg.pad, "alpha", x, 0.0, leg.to.alpha, at));
        }
        if let Some(cover) = &x.cover {
            // Up to full by the cut, down again after it. A clip that is keyed
            // carries its own coverage in its luma and this alpha is only what
            // brings it on and takes it away.
            curves.push(Curve {
                pad: cover.pad.clone(),
                property: "alpha",
                points: vec![
                    (x.start, 0.0),
                    (x.start + gst::ClockTime::from_nseconds(cut_ns), 1.0),
                    (x.end(), 0.0),
                ],
            });
        }
        curves
    }
}

// ---------------------------------------------------------------------------
// Curve builders
// ---------------------------------------------------------------------------

/// Smooth at both ends, the one easing this build has.
pub fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A property eased from one value to another over the whole window.
pub fn ramp(pad: &gst::Pad, property: &'static str, x: &Crossing, from: f64, to: f64) -> Curve {
    Curve { pad: pad.clone(), property, points: sample(x, |t| from + (to - from) * ease(t)) }
}

/// A property that holds `from`, changes on one frame at `at` (a fraction of
/// the window) and then holds `to`.
///
/// Two points a microsecond apart rather than one: a control source is a
/// function of time and needs a value on both sides of the step, and a
/// microsecond is far inside one frame at any frame rate anybody runs.
fn step(
    pad: &gst::Pad,
    property: &'static str,
    x: &Crossing,
    from: f64,
    to: f64,
    at: f64,
) -> Curve {
    let when = x.start
        + gst::ClockTime::from_nseconds(
            (x.duration.nseconds() as f64 * at.clamp(0.0, 1.0)) as u64,
        );
    let just_before = when.checked_sub(gst::ClockTime::from_useconds(1)).unwrap_or(x.start);
    Curve {
        pad: pad.clone(),
        property,
        points: vec![(x.start, from), (just_before, from), (when, to), (x.end(), to)],
    }
}

/// Every geometry property from one pad state to another, plus the alpha.
fn travel(pad: &gst::Pad, x: &Crossing, from: &PadState, to: &PadState) -> Vec<Curve> {
    let mut out = vec![ramp(pad, "alpha", x, from.alpha.max(0.0), to.alpha)];
    for (property, a, b) in [
        ("xpos", from.xpos as f64, to.xpos as f64),
        ("ypos", from.ypos as f64, to.ypos as f64),
        ("width", from.width.max(1) as f64, to.width.max(1) as f64),
        ("height", from.height.max(1) as f64, to.height.max(1) as f64),
    ] {
        if (a - b).abs() > f64::EPSILON {
            out.push(ramp(pad, property, x, a, b));
        }
    }
    out
}

/// Sample a shape over the window.
fn sample(x: &Crossing, f: impl Fn(f64) -> f64) -> Vec<(gst::ClockTime, f64)> {
    let ms = x.duration.mseconds().max(1);
    let steps = ((ms * SAMPLES_PER_SECOND) / 1000).clamp(2, 1024);
    (0..=steps)
        .map(|i| {
            let t = i as f64 / steps as f64;
            let at = x.start + gst::ClockTime::from_nseconds((x.duration.nseconds() as f64 * t) as u64);
            (at, f(t))
        })
        .collect()
}

/// Turn a plugin's per frame answers into curves.
///
/// `samples` is `(progress, pad name, property, value)` as the plugin gave
/// them, in progress order. What the core does with them is what it does with
/// its own: bind them and let the aggregator read them. A plugin is therefore
/// exactly as accurate as a built in, without having to be in the process.
pub fn from_samples(x: &Crossing, samples: &[(f64, String, String, f64)]) -> Vec<Curve> {
    let mut curves: Vec<Curve> = Vec::new();
    for (progress, pad_name, property, value) in samples {
        let Some(property) = DRIVEN.iter().find(|p| *p == property) else { continue };
        let Some(pad) = x
            .out
            .iter()
            .chain(&x.incoming)
            .chain(x.cover.iter())
            .find(|l| l.id() == *pad_name)
            .map(|l| l.pad.clone())
        else {
            continue;
        };
        let at = x.start
            + gst::ClockTime::from_nseconds(
                (x.duration.nseconds() as f64 * progress.clamp(0.0, 1.0)) as u64,
            );
        match curves.iter_mut().find(|c| c.pad == pad && c.property == *property) {
            Some(curve) => curve.points.push((at, *value)),
            None => curves.push(Curve { pad, property, points: vec![(at, *value)] }),
        }
    }
    for curve in &mut curves {
        curve.points.sort_by_key(|(t, _)| *t);
    }
    curves
}

// ---------------------------------------------------------------------------
// Transitions that live in a plugin
// ---------------------------------------------------------------------------

/// What the core calls to reach a `transition` plugin.
///
/// The mixer owns pipelines, not processes, so it never launches one of these:
/// the plugin supervisor installs a renderer and the mixer asks it by name.
/// A core with no plugins has none, and a take naming one gets an error that
/// lists the built in transitions.
pub trait Renderer: Send + Sync {
    /// One `render` call, in the protocol's own shape. `Err` is a plugin that
    /// is not there, is not running, or did not answer in time.
    fn render(&self, plugin: &str, request: &RenderRequest) -> anyhow::Result<serde_json::Value>;
}

/// The params of `render`, as `docs/reference/plugin-protocol.md` has them.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RenderRequest {
    pub from: Vec<String>,
    pub to: Vec<String>,
    pub progress: f64,
    pub running_time_ns: u64,
}

/// Ask a plugin what every pad should be, once per frame of the window.
///
/// Sampled before the window rather than during it. The plugin sees exactly
/// the progress and running time it would have seen frame by frame, and what
/// it answers becomes control points the aggregator reads on the frame, so a
/// wipe written in Python lands as precisely as a built in fade. The cost is
/// paid once, up front, inside `budget`; a plugin slower than that is not one
/// a take waits for.
///
/// A plugin may answer once with `{"curve": [[t, progress], ...]}` instead. It
/// is then a shape rather than a set of pad values, and the core applies it as
/// a crossfade eased by that shape, which is what a transition with nothing to
/// say about geometry wants.
pub fn sample_plugin(
    x: &Crossing,
    budget: Duration,
    mut render: impl FnMut(&RenderRequest) -> anyhow::Result<serde_json::Value>,
) -> anyhow::Result<Vec<Curve>> {
    let from: Vec<String> = x.out.iter().map(Leg::id).collect();
    let to: Vec<String> = x.incoming.iter().map(Leg::id).collect();
    let ms = x.duration.mseconds().max(1);
    let frames = ((ms * SAMPLES_PER_SECOND) / 1000).clamp(2, MAX_PLUGIN_SAMPLES);
    let started = std::time::Instant::now();
    let mut samples: Vec<(f64, String, String, f64)> = Vec::new();
    for i in 0..=frames {
        let progress = i as f64 / frames as f64;
        let request = RenderRequest {
            from: from.clone(),
            to: to.clone(),
            progress,
            running_time_ns: (x.start
                + gst::ClockTime::from_nseconds(
                    (x.duration.nseconds() as f64 * progress) as u64,
                ))
            .nseconds(),
        };
        let answer = render(&request)?;
        if let Some(curve) = answer.get("curve") {
            return Ok(shaped(x, curve));
        }
        collect(&answer, progress, &mut samples);
        if started.elapsed() > budget {
            warn!(
                spent_ms = started.elapsed().as_millis() as u64,
                sampled = i,
                of = frames,
                "the transition plugin ran out of budget; the rest of the curve is a straight line"
            );
            break;
        }
    }
    Ok(from_samples(x, &samples))
}

/// The most points a plugin is asked for, whatever the duration says. A long
/// stinger does not need six hundred round trips to look smooth.
const MAX_PLUGIN_SAMPLES: u64 = 64;

/// Pull `{pads: {id: {alpha, xpos, ...}}}` into flat samples.
fn collect(answer: &serde_json::Value, progress: f64, out: &mut Vec<(f64, String, String, f64)>) {
    let Some(pads) = answer.get("pads").and_then(|p| p.as_object()) else { return };
    for (pad, values) in pads {
        let Some(values) = values.as_object() else { continue };
        for (property, value) in values {
            if let Some(v) = value.as_f64() {
                out.push((progress, pad.clone(), property.clone(), v));
            }
        }
    }
}

/// A plugin's `{curve}`: a shape for a crossfade, as `[[t, progress], ...]`
/// with both in 0 to 1.
fn shaped(x: &Crossing, curve: &serde_json::Value) -> Vec<Curve> {
    let mut points: Vec<(f64, f64)> = Vec::new();
    if let Some(list) = curve.as_array() {
        for entry in list {
            let pair = entry.as_array().map(|p| p.as_slice()).unwrap_or(&[]);
            if let [t, v] = pair {
                if let (Some(t), Some(v)) = (t.as_f64(), v.as_f64()) {
                    points.push((t.clamp(0.0, 1.0), v.clamp(0.0, 1.0)));
                }
            }
        }
    }
    if points.len() < 2 {
        return Vec::new();
    }
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    let at = |t: f64| -> f64 {
        let mut last = points[0];
        for p in &points {
            if p.0 > t {
                let span = p.0 - last.0;
                if span <= 0.0 {
                    return p.1;
                }
                return last.1 + (p.1 - last.1) * ((t - last.0) / span);
            }
            last = *p;
        }
        last.1
    };
    let mut curves = Vec::new();
    for leg in &x.out {
        let alpha = leg.from.alpha;
        curves.push(Curve {
            pad: leg.pad.clone(),
            property: "alpha",
            points: sample(x, |t| alpha * (1.0 - at(t))),
        });
    }
    for leg in &x.incoming {
        let alpha = leg.to.alpha;
        curves.push(Curve {
            pad: leg.pad.clone(),
            property: "alpha",
            points: sample(x, |t| alpha * at(t)),
        });
    }
    curves
}

/// The volume curves for a crossing, on the audiomixer pads.
///
/// One pad per source however many places its picture is drawn in, because a
/// source heard twice is not louder. A pad already where it is going is left
/// alone rather than bound and unbound for nothing.
pub fn audio_curves(x: &Crossing) -> Vec<Curve> {
    x.audio
        .iter()
        .filter(|(_, from, to)| (from - to).abs() > 1e-6)
        .map(|(pad, from, to)| Curve {
            pad: pad.clone(),
            property: "volume",
            points: sample(x, |t| from + (to - from) * ease(t)),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Binding
// ---------------------------------------------------------------------------

/// Every control binding this mixer has put on a compositor pad.
///
/// One binding per pad and property, made the first time a transition drives
/// it and then left where it is for as long as the pad is on the element. A
/// transition loads its curve into the control source the binding already
/// reads and turns the binding on; settling turns it off again. Nothing is
/// ever taken off a pad while the pipeline is running.
///
/// That rule is not tidiness, it is the only thing keeping the aggregator
/// alive. `gst_object_sync_values` walks the pad's list of bindings with the
/// object lock commented out (`gstobject.c` says "FIXME: this deadlocks") and
/// takes no reference to the binding it is about to call. Every frame, for
/// every pad. `gst_object_remove_control_binding` takes that lock, frees the
/// list node under it and unparents the binding, which was the compositor's
/// last reference to it. Do that from the mixer thread and the aggregator
/// reads a freed node and calls a freed object, which is the SIGSEGV in
/// `gst_object_sync_values` this design exists to remove.
///
/// What is left is safe against the same unlocked walk:
///
/// * `disabled` is a plain boolean the walk reads and nothing frees. The
///   binding stays in the list either way.
/// * an `InterpolationControlSource`'s timed values are behind that source's
///   own mutex, which its `get_value` takes. Rewriting the curve under a
///   running aggregator is what the type is for.
/// * the binding holds a strong reference to its control source, so the walk
///   cannot meet a freed one.
///
/// A pad the compositor has given back is dropped from here on the next bind.
/// That drops a reference and nothing else: the binding stays parented to the
/// pad and is freed with it, and by then the pad is off the element and no
/// aggregator can be walking it.
#[derive(Default)]
pub struct Controllers {
    entries: Vec<Entry>,
}

/// One pad, one property, and the two objects that drive it for good.
struct Entry {
    pad: gst::Pad,
    property: &'static str,
    binding: gst::ControlBinding,
    source: InterpolationControlSource,
}

impl Controllers {
    /// Bind a set of curves onto their pads.
    ///
    /// A pad that refuses a binding is not an error: its curve goes back in
    /// `unbound` and the caller runs it the old way. That is how a GPU
    /// compositor whose pad does not declare a property controllable keeps
    /// working.
    pub fn bind(&mut self, curves: Vec<Curve>) -> Bound {
        self.forget_released_pads();
        let mut driven = Vec::new();
        let mut unbound = Vec::new();
        for curve in curves {
            if curve.points.len() < 2 {
                unbound.push(curve);
                continue;
            }
            if !curve.pad.has_property(curve.property) {
                warn!(pad = %curve.pad.name(), property = curve.property, "this pad has no such property");
                continue;
            }
            let Some(entry) = self.controller(&curve.pad, curve.property) else {
                debug!(
                    pad = %curve.pad.name(),
                    property = curve.property,
                    "this pad would not take a control binding; the property thread will run it"
                );
                unbound.push(curve);
                continue;
            };
            // Off while the curve is written, so the aggregator never blends a
            // frame against half of it, and on once it is whole.
            entry.binding.set_disabled(true);
            entry.source.unset_all();
            for (at, value) in &curve.points {
                entry.source.set(*at, *value);
            }
            entry.binding.set_disabled(false);
            driven.push(Driven {
                pad: curve.pad.clone(),
                property: curve.property,
                binding: entry.binding.clone(),
                source: entry.source.clone(),
                value: curve.settle(),
            });
        }
        Bound { driven, unbound }
    }

    /// How many pads and properties this mixer has ever driven. For a test
    /// that has to prove the bindings are made once rather than per take.
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// The binding for this pad and property, made if this is the first time.
    ///
    /// It is made disabled, with an empty control source, because the pad is
    /// already on a compositor that syncs every binding it holds: an enabled
    /// one would write whatever the empty source said on the very next frame.
    fn controller(&mut self, pad: &gst::Pad, property: &'static str) -> Option<&Entry> {
        if let Some(i) =
            self.entries.iter().position(|e| &e.pad == pad && e.property == property)
        {
            return Some(&self.entries[i]);
        }
        let source = InterpolationControlSource::new();
        source.set_mode(InterpolationMode::Linear);
        let binding =
            gstreamer_controller::DirectControlBinding::new_absolute(pad, property, &source);
        binding.set_disabled(true);
        if let Err(e) = pad.add_control_binding(&binding) {
            debug!(pad = %pad.name(), property, ?e, "this pad refused a control binding");
            return None;
        }
        self.entries.push(Entry {
            pad: pad.clone(),
            property,
            binding: binding.upcast(),
            source,
        });
        self.entries.last()
    }

    /// Drop what belongs to a pad the compositor has taken back.
    fn forget_released_pads(&mut self) {
        self.entries.retain(|e| e.pad.parent().is_some());
    }
}

/// A transition that is running: what it drives, and where each property is to
/// be left when it ends.
pub struct Bound {
    driven: Vec<Driven>,
    /// Curves no pad would take. The property thread runs these instead, which
    /// is the only thing it is still for.
    pub unbound: Vec<Curve>,
}

/// One property this transition owns until it settles.
struct Driven {
    pad: gst::Pad,
    property: &'static str,
    binding: gst::ControlBinding,
    source: InterpolationControlSource,
    value: f64,
}

impl Bound {
    pub fn is_empty(&self) -> bool {
        self.driven.is_empty() && self.unbound.is_empty()
    }

    pub fn len(&self) -> usize {
        self.driven.len()
    }

    /// Whether this property of this pad is being driven right now.
    ///
    /// The supervisor's visibility tick runs twice a second and writes every
    /// pad it can see. A write to a property under a live binding is undone by
    /// the next sync anyway, but it also makes the picture jump for one frame,
    /// so the tick asks first.
    pub fn drives(&self, pad: &gst::Pad, property: &str) -> bool {
        self.driven.iter().any(|d| &d.pad == pad && d.property == property)
    }

    /// Hand every property back and leave each one where its curve ended.
    ///
    /// The curve is collapsed to that one value before the binding is turned
    /// off, so a sync already in flight on the aggregator thread computes the
    /// same number this thread is about to write and the two cannot disagree.
    /// A transition cut short by a newer take therefore lands on its
    /// destination rather than wherever it had got to.
    pub fn settle(self) {
        for d in self.driven {
            d.source.unset_all();
            d.source.set(gst::ClockTime::ZERO, d.value);
            d.binding.set_disabled(true);
            write(&d.pad, d.property, d.value);
        }
    }
}

/// Write a curve's value at a fraction of its window, for the fallback thread.
///
/// The curve is in running time and the thread only knows how far through it
/// is, so the fraction is read against the curve's own span rather than
/// against a clock. That keeps the fallback and the binding describing the
/// same shape even though only one of them lands on a frame.
pub fn write_at(curve: &Curve, t: f64) {
    let Some((first, _)) = curve.points.first() else { return };
    let Some((last, _)) = curve.points.last() else { return };
    let span = last.nseconds().saturating_sub(first.nseconds()) as f64;
    let at = gst::ClockTime::from_nseconds(first.nseconds() + (span * t.clamp(0.0, 1.0)) as u64);
    let mut previous = curve.points[0];
    let mut value = previous.1;
    for point in &curve.points {
        if point.0 > at {
            let gap = (point.0.nseconds() - previous.0.nseconds()) as f64;
            value = match gap > 0.0 {
                true => {
                    let f = (at.nseconds() - previous.0.nseconds()) as f64 / gap;
                    previous.1 + (point.1 - previous.1) * f
                }
                false => point.1,
            };
            break;
        }
        previous = *point;
        value = point.1;
    }
    write(&curve.pad, curve.property, value);
}

/// Write one of the driven properties, whatever type the pad holds it as.
///
/// A compositor pad's `width` is an int and its `alpha` a double, and setting
/// one with the wrong Rust type panics inside glib rather than failing.
fn write(pad: &gst::Pad, property: &str, value: f64) {
    match property {
        "alpha" => pad.set_property(property, value.clamp(0.0, 1.0)),
        "volume" => pad.set_property(property, value.clamp(0.0, 10.0)),
        "width" | "height" => pad.set_property(property, (value.round() as i32).max(1)),
        _ => pad.set_property(property, value.round() as i32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        let _ = gst::init();
    }

    /// A compositor with two pads, standing in for the slot pool.
    fn pads(n: usize) -> (gst::Element, Vec<gst::Pad>) {
        init();
        let comp = crate::gstutil::make("compositor", "tx-test").expect("compositor");
        let pads = (0..n)
            .map(|_| comp.request_pad_simple("sink_%u").expect("a pad"))
            .collect();
        (comp, pads)
    }

    fn state(alpha: f64) -> PadState {
        PadState { xpos: 0, ypos: 0, width: 1920, height: 1080, alpha }
    }

    fn crossing(out: Vec<gst::Pad>, incoming: Vec<gst::Pad>) -> Crossing {
        Crossing {
            out: out
                .into_iter()
                .map(|pad| Leg {
                    pad,
                    item: None,
                    source: "cam1".into(),
                    from: state(1.0),
                    to: state(0.0),
                })
                .collect(),
            incoming: incoming
                .into_iter()
                .map(|pad| Leg {
                    pad,
                    item: None,
                    source: "cam2".into(),
                    from: state(0.0),
                    to: state(1.0),
                })
                .collect(),
            audio: Vec::new(),
            cover: None,
            start: gst::ClockTime::from_seconds(10),
            duration: gst::ClockTime::from_mseconds(300),
        }
    }

    /// The whole point: the value is a function of the frame's running time,
    /// so it is the same every run and on every machine.
    #[test]
    fn a_fade_is_half_way_half_way_through() {
        let (comp, pads) = pads(2);
        let x = crossing(vec![pads[0].clone()], vec![pads[1].clone()]);
        let curves = Fade.curves(&x);
        assert_eq!(curves.len(), 2);
        let mid = x.start + x.duration / 2;
        for curve in &curves {
            let at = curve
                .points
                .iter()
                .min_by_key(|(t, _)| t.nseconds().abs_diff(mid.nseconds()))
                .expect("a sampled curve has points");
            assert!(
                (at.1 - 0.5).abs() < 0.02,
                "{} was {} half way through, not 0.5",
                curve.property,
                at.1
            );
        }
        // And the ends are exactly the ends: an outgoing pad reaches nothing
        // and an incoming one reaches the alpha its scene asked for.
        assert_eq!(curves[0].points.last().expect("points").1, 0.0);
        assert_eq!(curves[1].points.last().expect("points").1, 1.0);
        for pad in &pads {
            comp.release_request_pad(pad);
        }
    }

    #[test]
    fn a_cut_has_no_curves_to_bind() {
        let (comp, pads) = pads(2);
        let x = crossing(vec![pads[0].clone()], vec![pads[1].clone()]);
        assert!(Cut.curves(&x).is_empty());
        for pad in &pads {
            comp.release_request_pad(pad);
        }
    }

    /// A stinger swaps the scenes on one frame rather than dissolving them,
    /// and the clip is up at that point.
    #[test]
    fn a_stinger_cuts_under_the_clip_rather_than_mixing() {
        let (comp, pads) = pads(3);
        let mut x = crossing(vec![pads[0].clone()], vec![pads[1].clone()]);
        x.cover = Some(Leg {
            pad: pads[2].clone(),
            item: None,
            source: "__stinger__".into(),
            from: state(0.0),
            to: state(0.0),
        });
        let curves = Stinger { cut_at_ms: None }.curves(&x);
        let incoming = curves
            .iter()
            .find(|c| c.pad == pads[1] && c.property == "alpha")
            .expect("the incoming scene has a curve");
        // A quarter of the way through it is still nothing: the swap is a
        // step, not a ramp.
        let quarter = x.start + x.duration / 4;
        let at = value_at(incoming, quarter);
        assert_eq!(at, 0.0, "the incoming scene must not fade up under a stinger");
        let after = value_at(incoming, x.start + x.duration * 3 / 4);
        assert_eq!(after, 1.0, "the incoming scene must be up after the cut point");
        let cover = curves
            .iter()
            .find(|c| c.pad == pads[2])
            .expect("the clip has a curve");
        assert_eq!(value_at(cover, x.start + x.duration / 2), 1.0, "the clip covers the cut");
        for pad in &pads {
            comp.release_request_pad(pad);
        }
    }

    /// Reading a curve the way the aggregator will: the last point at or
    /// before the time asked for, interpolated onto the next.
    fn value_at(curve: &Curve, at: gst::ClockTime) -> f64 {
        let mut last = curve.points[0];
        for point in &curve.points {
            if point.0 > at {
                let span = (point.0 - last.0).nseconds() as f64;
                if span <= 0.0 {
                    return point.1;
                }
                let t = (at - last.0).nseconds() as f64 / span;
                return last.1 + (point.1 - last.1) * t;
            }
            last = *point;
        }
        last.1
    }

    #[test]
    fn a_move_travels_the_item_that_is_in_both_scenes() {
        let (comp, pads) = pads(2);
        let item = Id::new();
        let mut x = crossing(vec![pads[0].clone()], vec![pads[1].clone()]);
        x.out[0].item = Some(item);
        x.out[0].from = PadState { xpos: 1280, ypos: 620, width: 480, height: 270, alpha: 1.0 };
        x.incoming[0].item = Some(item);
        x.incoming[0].to = PadState { xpos: 0, ypos: 0, width: 1920, height: 1080, alpha: 1.0 };
        let curves = Move.curves(&x);
        let moved: Vec<&str> = curves
            .iter()
            .filter(|c| c.pad == pads[1])
            .map(|c| c.property)
            .collect();
        for property in ["alpha", "xpos", "ypos", "width", "height"] {
            assert!(moved.contains(&property), "a move must drive {property}: {moved:?}");
        }
        let x_curve = curves
            .iter()
            .find(|c| c.pad == pads[1] && c.property == "xpos")
            .expect("an xpos curve");
        let mid = value_at(x_curve, x.start + x.duration / 2);
        assert!(
            mid > 100.0 && mid < 1200.0,
            "half way through the inset should be between its two homes, was {mid}"
        );
        for pad in &pads {
            comp.release_request_pad(pad);
        }
    }

    #[test]
    fn an_item_in_both_scenes_is_matched_by_id_before_source() {
        let (comp, pads) = pads(4);
        let a = Id::new();
        let mut x = crossing(vec![pads[0].clone(), pads[1].clone()], vec![
            pads[2].clone(),
            pads[3].clone(),
        ]);
        x.out[0].item = Some(a);
        x.out[0].source = "cam1".into();
        x.out[1].source = "cam2".into();
        // The incoming scene has the same source in the other order, and the
        // matching item id on the second entry.
        x.incoming[0].source = "cam1".into();
        x.incoming[1].item = Some(a);
        x.incoming[1].source = "cam9".into();
        let pairs = x.matched();
        assert!(pairs.contains(&(0, 1)), "the item id must win over the source: {pairs:?}");
        for pad in &pads {
            comp.release_request_pad(pad);
        }
    }

    /// The binding itself, on a real compositor pad, because the whole design
    /// rests on the pad taking one.
    ///
    /// And on the pad keeping it. Settling disables the binding rather than
    /// removing it: `gst_object_sync_values` walks the pad's bindings on the
    /// aggregator thread with no lock and no reference, so a removal from this
    /// thread frees a list node and an object out from under that walk.
    #[test]
    fn a_compositor_pad_takes_a_control_binding_and_keeps_it() {
        let (comp, pads) = pads(1);
        let x = crossing(vec![pads[0].clone()], Vec::new());
        let mut controllers = Controllers::default();
        let bound = controllers.bind(Fade.curves(&x));
        assert_eq!(bound.len(), 1, "a compositor pad must take an alpha binding");
        assert!(bound.unbound.is_empty(), "nothing should have fallen back");
        assert!(bound.drives(&pads[0], "alpha"));
        assert!(!bound.drives(&pads[0], "xpos"));
        let binding = pads[0].control_binding("alpha").expect("the pad is holding it");
        assert!(!binding.is_disabled(), "and reading it while the transition runs");
        bound.settle();
        assert!(
            pads[0].control_binding("alpha").expect("still there").is_disabled(),
            "settle must hand the property back without taking the binding off"
        );
        assert_eq!(pads[0].property::<f64>("alpha"), 0.0, "settle leaves the curve's last value");

        // A second transition on the same pad reuses the binding it made the
        // first time rather than adding another one.
        let again = controllers.bind(Fade.curves(&x));
        assert_eq!(controllers.count(), 1, "one binding per pad and property, ever");
        assert!(!pads[0].control_binding("alpha").expect("still there").is_disabled());
        again.settle();
        comp.release_request_pad(&pads[0]);
    }

    /// A pad the compositor has taken back is forgotten, or the pool would
    /// grow one dead pad per source for the length of a show.
    #[test]
    fn a_released_pad_is_forgotten_on_the_next_bind() {
        let (comp, pads) = pads(2);
        let mut controllers = Controllers::default();
        let first = crossing(vec![pads[0].clone()], Vec::new());
        controllers.bind(Fade.curves(&first)).settle();
        assert_eq!(controllers.count(), 1);
        comp.release_request_pad(&pads[0]);
        let second = crossing(vec![pads[1].clone()], Vec::new());
        controllers.bind(Fade.curves(&second)).settle();
        assert_eq!(controllers.count(), 1, "the released pad's entry went with it");
        comp.release_request_pad(&pads[1]);
    }

    #[test]
    fn a_plugins_samples_become_the_same_curves_a_built_in_makes() {
        let (comp, pads) = pads(1);
        let x = crossing(Vec::new(), vec![pads[0].clone()]);
        let name = pads[0].name().to_string();
        let samples = vec![
            (0.0, name.clone(), "xpos".to_string(), -1920.0),
            (0.5, name.clone(), "xpos".to_string(), -960.0),
            (1.0, name.clone(), "xpos".to_string(), 0.0),
            // A property no transition may drive is dropped rather than
            // written: a plugin cannot reach `zorder` and reorder the canvas.
            (0.5, name.clone(), "zorder".to_string(), 4.0),
        ];
        let curves = from_samples(&x, &samples);
        assert_eq!(curves.len(), 1, "one property, one curve: {curves:?}");
        assert_eq!(curves[0].property, "xpos");
        assert_eq!(curves[0].points.len(), 3);
        assert_eq!(value_at(&curves[0], x.start + x.duration / 2), -960.0);
        comp.release_request_pad(&pads[0]);
    }

    #[test]
    fn a_zero_duration_is_a_cut_whatever_the_type_says() {
        assert!(TransitionSpec { kind: Kind::Fade, duration_ms: 0 }.is_cut());
        assert!(!TransitionSpec { kind: Kind::Fade, duration_ms: 300 }.is_cut());
        assert!(TransitionSpec::cut().is_cut());
        assert_eq!(
            TransitionSpec { kind: Kind::Fade, duration_ms: 99_000 }.duration(),
            Duration::from_millis(MAX_DURATION_MS),
            "a transition longer than the ceiling is held at it"
        );
    }
}
