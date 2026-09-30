//! The rendition planner. Given what every output asks for and what every
//! source carries, it returns the smallest graph that serves them all, and
//! the difference between two such graphs. It decides and never acts: no
//! GStreamer, no threads, no clock. The graph builder in the core acts on
//! what it returns.
//!
//! # The surface
//!
//! * [`plan`]`(sources, requests, model) -> Result<Plan, PlanError>`.
//!   `sources` pairs a source's slug with its [`StreamInfo`]; `requests`
//!   pairs the slug an output reads with its [`RenditionRequest`].
//! * [`Plan`]: [`Node`]s in start order, each with a stable id, its
//!   [`NodeKind`] (Source, Copy, Decode, Scale, Encode, AudioConvert,
//!   AudioEncode, Mux), its inputs, the requests it serves, the device its
//!   [`Cost`] is counted on and, for Encode and Mux, a [`Reason`]. Plus the
//!   keyframe interval each source's ladder uses and the cost per device.
//! * [`diff`]`(old, new) -> PlanDiff`: node ids to start, stop, restart and
//!   keep. Adding a rung to a ladder starts its Scale, Encode and Mux and
//!   keeps everything else.
//! * [`CostModel`]: the planner's only view of the machine. The governor
//!   implements it from calibration; [`StaticCostModel`] is fixed numbers
//!   for tests and for a machine that has not been calibrated.
//!
//! # The rules
//!
//! 1. Copy when the source already matches: encoded, same codec, size and
//!    frame rate, bitrate within the request's tolerance (25% when unset),
//!    keyframe interval equal when both are known, and the container can
//!    carry the codec. A copy into a container that cannot carry the source
//!    codec becomes an encode.
//! 2. Decode at most once per source and track.
//! 3. Scale once per distinct size and frame rate of a source, and not at
//!    all when both match the source.
//! 4. Encode once per distinct (codec, size, rate, bitrate, keyframe
//!    interval) of a source; every request that wants it shares it.
//! 5. Every encode of one source uses one keyframe interval: the shortest
//!    any rung asked for, 2000 ms when none did.
//! 6. Audio follows the same rules: copy, one decode, one convert per
//!    distinct rate and channel count, one encode per distinct shape.
//! 7. Encoder choice: hardware of the right codec, then software, in the
//!    model's order, never one whose device has no room left for this plan.
//!    The Encode node's reason says which and why.
//! 8. A request nothing here can satisfy is a [`PlanError`] that names what
//!    is missing and, where there is one, the nearest shape that works.
//!
//! Node ids are built from what a node does (`encode:cam:h264:1280x720p30:
//! 2800k:g2000`), never from a counter, which is what lets [`diff`] match
//! nodes across plans. `docs/reference/renditions.md` has the full list.

mod audio;
mod build;
mod chain;
mod choose;
pub mod container;
mod diff;
mod error;
mod graph;
mod ids;
mod model;
mod nearest;
mod plan;
mod resolve;
mod sizing;
mod static_model;

pub use diff::{diff, PlanDiff};
pub use error::{PlanError, Skip, Suggestion};
pub use graph::{Node, NodeKind, Plan, Reason, ReasonCode, Track};
pub use model::{device_of, AudioWork, CostModel, Room, CPU, GPU};
pub use plan::{plan, SourceId, DEFAULT_KEYFRAME_MS};
pub use resolve::DEFAULT_TOLERANCE;
pub use static_model::StaticCostModel;

pub use godwinmix_protocol::rendition::{
    AudioCodec, AudioShape, AudioWant, Container, Cost, EncoderSlot, Fps, RenditionRequest, StreamInfo,
    VideoCodec, VideoShape, VideoWant,
};
