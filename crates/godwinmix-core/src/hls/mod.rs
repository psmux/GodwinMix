//! HLS and LL-HLS out, served from the control port with no new port.
//!
//! # The seam with the graph builder
//!
//! An `hls/output` with a rendition makes no encoder of its own. The mixer
//! plans its rungs with every other output's ([`request::choice`] says what
//! it asks for: a ladder whose keyframes fall on every segment boundary), and
//! the output's kind is handed one [`crate::render::Tap`] per rung.
//! [`rungs`] packages them: the top rung arrives on the core's own feed, and
//! each lower one on a feed of its own off its tee.
//!
//! Anything else that has encoded pads can package them the same way:
//!
//! ```ignore
//! let stream = hls::stream::get("viewers").expect("the hls/output is up");
//! let rung = hls::attach(&pipeline, &stream, hls::Input {
//!     id: "720p",                 // the rung's slug, in its URL
//!     kind: hls::TrackKind::Video,
//!     pad: &tee.request_pad_simple("src_%u").unwrap(),
//!     declared_kbps: 3000,        // BANDWIDTH until one is measured
//! })?;
//! // Later, to take the rung away again:
//! rung.detach(&pipeline, &stream);
//! ```
//!
//! That is the whole contract: [`attach`] with an unlinked src pad carrying
//! H.264, HEVC, AV1, AAC or Opus, and [`package::Attached::detach`]. The
//! module finds the codec from the caps. It never asks the encoder for a
//! keyframe: every rung of one ladder must already have its keyframes on the
//! same frames, every `segment_ms` (`render::keyframes::align` does it for
//! the planner's encoders). The pads may be in the programme pipeline or any
//! other; the packager adds a leaky queue of its own, so a slow packager
//! drops its own frames and never holds up the encoder.
//!
//! # The parts
//!
//! * [`package`]: pad in, `cmafmux`, appsink, [`cutter`] out.
//! * [`ring`]: the segments and parts one rung keeps, as `Bytes` every viewer
//!   shares.
//! * [`track`]: a ring behind a mutex held for microseconds, and a `watch`
//!   channel a blocking playlist request awaits on the server's runtime.
//! * [`playlist`]: the multivariant and media playlists, pure functions.
//! * [`stream`]: one output's tracks and viewers, and the registry the
//!   control port finds them in.
//! * [`viewers`]: who fetched a segment lately, and the egress in kbit/s.
//!
//! `docs/explanation/hls-output.md` says why it is built this way and what
//! it costs.

pub mod boxes;
pub mod cutter;
pub mod dash;
pub mod key;
#[cfg(test)]
mod ladder;
pub mod master;
pub mod output;
pub mod package;
pub mod params;
pub mod parse;
pub mod playlist;
pub mod registry;
pub mod request;
pub mod ring;
pub mod rungs;
pub mod stream;
pub mod track;
pub mod view;
pub mod viewers;

pub use package::{attach, Attached, Input};
pub use params::HlsParams;
pub use stream::Stream;
pub use track::TrackKind;

#[cfg(test)]
mod tests;
