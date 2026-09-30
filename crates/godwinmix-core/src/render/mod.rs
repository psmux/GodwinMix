//! Renditions on the programme's outputs: the planner's graph, built in
//! GStreamer, with the governor asked before every encoder starts.
//!
//! An output with no `rendition` is what it always was: it hangs off the
//! programme encoder's tees and costs nothing more. An output that asks for a
//! rendition (a request or a preset) is planned with every other one by
//! `godwinmix_render::plan`, against the programme as a raw source, and only
//! the nodes the diff names are started or stopped. Five outputs that want
//! the same 720p get one encoder.
//!
//! # The seam other modules build on (HLS reads this)
//!
//! Each request an output makes is one rung. A plain rendition is one rung
//! whose request id is the output id; a ladder preset gives one rung per
//! step, `<output>-1080p`, `<output>-720p` and so on, top first.
//!
//! * [`Tap`]: one rung's encoded video and audio, as tees in the programme
//!   pipeline, with the shapes the plan gave them and the keyframe interval
//!   every rung of the ladder shares. Keyframes are forced at the same
//!   running times on every encoder of a ladder, so segments line up.
//! * `OutputCtx::taps`: an output's kind is handed its rungs when it builds,
//!   top first. The core's own feed (the queue and proxy the generic output
//!   code puts in front of `video` and `audio`) already reads rung 0.
//! * `Mixer::rendition_taps(output)` gives the same list on the mixer thread.
//! * [`Tap::feed`]`(name, queue_secs)` puts a leaky queue and a `proxysink`
//!   on a rung's tees inside the programme pipeline and returns them as a
//!   [`Feed`]; a `proxysrc` in the consumer's own pipeline reads it.
//!   [`Feed::detach`] takes it out again. A slow consumer drops its own
//!   buffers and never holds the encoder.
//! * A tee outlives its encoder being stopped by the governor or restarted
//!   by a replan: a feed stays linked, and data resumes (with a keyframe)
//!   when the encoder is back. The tee goes only when the last request
//!   using that rendition is removed.
//!
//! Nothing here runs on a streaming thread or the bus handler except the
//! keyframe probe, which does arithmetic and pushes one event.
//!
//! # The rest
//!
//! * `model`: the governor's profile as the planner's `CostModel`.
//! * `candidates`: the codec catalogue as calibration candidates.
//! * `station`: the governor for this process, sampling from start, with a
//!   calibration in the background on first run when nothing is on air.
//! * `elements`, `keyframes`, `graph`: node kinds to elements, the aligned
//!   keyframes, and applying a plan diff to a running pipeline.
//! * `admit`, `shed`: tickets per encoder, refusals with advice, and giving
//!   an encoder up when the machine runs short.

mod admit;
mod apply;
mod book;
pub mod candidates;
mod elements;
mod graph;
pub mod keyframes;
pub mod model;
mod refusal;
mod renditions;
mod shed;
pub mod station;
pub mod status;
mod tap;
mod view;
mod wiring;

pub use graph::Programme;
pub use refusal::Refusal;
pub use renditions::{Renditions, RenditionsHandle, PROGRAMME};
pub use shed::{Tick, RESTORE_AFTER};
pub use station::{Station, CALIBRATION_ENV};
pub use tap::{Feed, Tap};
