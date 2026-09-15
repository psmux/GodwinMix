//! The GodwinMix mixing engine, as a library.
//!
//! Multiple sources come in, one of them is on program at a time, and the
//! program feed goes out to one or more destinations without ever stopping.
//! Switching source is instant and does not disturb the outgoing stream,
//! because the output encoder is started once and runs for the life of the
//! broadcast; everything that changes happens upstream of it in raw video.
//!
//! See `mixer.rs` for why that arrangement is the whole design.
//!
//! ## What is here and what is not
//!
//! Everything that builds and runs pipelines: the mixer thread and its command
//! queue, sources, outputs, the plugin traits and the built in kinds, the codec
//! catalogue, scenes, multiview, snapshots, the media library, and the parts of
//! `observe` that instrument the engine.
//!
//! Nothing that serves a request. There is no HTTP server here, no JSON-RPC
//! dispatcher, no MCP server and no command line. Those live in the
//! `godwinmix` crate, which depends on this one. The wire types live in
//! `godwinmix-protocol`, which this crate depends on for the status and event
//! records it publishes.
//!
//! ## Embedding it
//!
//! ```no_run
//! use godwinmix_core::prelude::*;
//!
//! # fn demo() -> anyhow::Result<()> {
//! gstreamer::init()?;
//! // Every field has a default, so an empty document is a working mixer.
//! let cfg: Config = toml::from_str("")?;
//! let (mut mix, handle, cmd_rx, _bus_rx) = Mixer::build(cfg)?;
//! mix.start()?;
//! let thread = mixer::spawn(mix, cmd_rx, handle.clone());
//! handle.send(Command::Shutdown).ok();
//! let _ = thread.join();
//! # Ok(())
//! # }
//! ```
//!
//! `examples/embed.rs` is the same thing with a source added and a take, and
//! it is built and run in CI.

pub mod caps;
pub mod catalogue;
pub mod config;
pub mod convert;
pub mod encoder;
pub mod gstutil;
pub mod hooks;
pub mod input;
pub mod media;
pub mod mixer;
pub mod multiview;
pub mod observe;
pub mod output;
pub mod plugin;
pub mod preview;
pub mod preset;
pub mod probe;
pub mod safety;
pub mod scene;
pub mod snapshot;
pub mod telemetry;
pub mod state;
pub mod tasks;
pub mod zip;

/// The handful of names a program that embeds the engine reaches for first.
///
/// Deliberately small. Everything else is one `use godwinmix_core::...` away,
/// and a prelude that re-exported the world would make the layering harder to
/// read rather than easier.
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::mixer::{self, Command, Mixer, MixerHandle};
    pub use crate::state::{Event, MixerStatus, SourceState};
}
