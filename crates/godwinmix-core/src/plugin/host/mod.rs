//! Tier 2 in the engine: the five trait implementations that make a process
//! look like a built in kind.
//!
//! ```text
//!   process.rs    spawn, handshake, talk, poll, stop; the control channel
//!   transport.rs  the media end per transport, and where its sockets live
//!   source.rs     SidecarSource:  the same MediaEnds every other source makes
//!   output.rs     SidecarOutput:  the programme, muxed, down a FIFO
//!   filter.rs     SidecarFilter:  raw out and raw back on two sockets
//!   service.rs    SidecarService and SidecarDevice: no media at all
//! ```
//!
//! The split with `godwinmix-host` is the split between a pipeline and a
//! protocol. That crate knows what to say, what a line means, what state an
//! instance is in and what it costs; this module knows how to spawn with the
//! core's own process group teardown and how to turn the result into
//! GStreamer. Neither knows about the other's half.
//!
//! Nothing in here is a privilege a third party does not have. A sidecar
//! reaches the mixer through `Source`, `Output` and `Filter`, which is the
//! same door `rtmp/source` uses.

pub mod filter;
pub mod output;
pub mod process;
pub mod service;
pub mod source;
pub mod transport;

pub use filter::SidecarFilter;
pub use output::SidecarOutput;
pub use process::{Notice, Sidecar};
pub use service::{SidecarDevice, SidecarService};
pub use source::{SidecarSource, SidecarSpec};
pub use transport::MediaDir;
