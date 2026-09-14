//! Write a GodwinMix plugin in Rust.
//!
//! A plugin is a process. It talks JSON-RPC 2.0 over stdin and stderr, one
//! object per line, and it puts media on stdout or on a socket. That is the
//! whole contract, and it is small enough that `examples/zero-dep-source.py` in
//! the repository implements it in under two hundred lines of Python with no
//! dependencies at all. This crate exists to save a Rust author from writing
//! the same loop again, not because the loop is hard.
//!
//! # A source in one page
//!
//! ```no_run
//! use godwinmix_sdk::prelude::*;
//!
//! struct Grey {
//!     canvas: Canvas,
//!     media: Option<VideoLoop>,
//! }
//!
//! impl Source for Grey {
//!     fn initialize(&mut self, ready: &Ready, _r: Reporter) -> Result<InitializeResult, RpcError> {
//!         self.canvas = ready.canvas;
//!         Ok(InitializeResult { latency_ms: Some(0) })
//!     }
//!
//!     fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
//!         let canvas = self.canvas;
//!         let writer = media::open(params.transport, &params.media, canvas,
//!                                  media::Streams::video_only(media::VideoFormat::I420))
//!             .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))?;
//!         self.media = Some(VideoLoop::spawn(canvas, writer, None, move |frame, _pts| {
//!             let luma = canvas.width as usize * canvas.height as usize;
//!             frame[..luma].fill(128);
//!             frame[luma..].fill(128);
//!         }));
//!         Ok(StartResult::default())
//!     }
//!
//!     fn stop(&mut self) -> Result<(), RpcError> {
//!         self.media = None;
//!         Ok(())
//!     }
//!
//!     fn configure(&mut self, _params: serde_json::Value) -> Result<Configure, RpcError> {
//!         Ok(Configure::applied())
//!     }
//! }
//!
//! fn main() {
//!     let env = PluginEnv::from_env();
//!     let manifest = Manifest::load(env.root.join("gmx-plugin.toml")).expect("gmx-plugin.toml");
//!     let plugin = Grey { canvas: Canvas::default(), media: None };
//!     runtime::run(&manifest, SourceHandler(plugin)).expect("the plugin stopped badly");
//! }
//! ```
//!
//! # What is in here
//!
//! | Module | What it is for |
//! |---|---|
//! | [`wire`] | every type that crosses the pipe, and nothing else |
//! | [`framing`] | JSON lines, the 4 MiB limit, ids per direction |
//! | [`handshake`] | the `initialize` exchange and the legal call order per state |
//! | [`manifest`] | `gmx-plugin.toml`: types, parser, and a validator that reports every problem with its key path |
//! | [`skill`] | `SKILL.md` frontmatter, and the check the harness runs |
//! | [`plugin`] | the `Source`, `Output`, `Filter`, `Service`, `Device` and `Transition` traits |
//! | [`runtime`] | the two thread main loop |
//! | [`media`] | the container transport in pure Rust, and `unixfd`/`shm` behind the `gst` feature |
//! | [`pacing`] | the frame pool and the pacer |
//! | [`driver`] | `VideoLoop`, which turns a `draw` closure into a source |
//! | [`env`] | the `GMX_*` variables, read once |
//! | [`crash`] | a panic hook that leaves a report the core can attach to an alert |
//! | [`transcript`] | the recorded transcript format for offline tests |
//!
//! # Features
//!
//! Default: nothing. The crate depends on serde, serde_json and toml, and
//! writes Matroska itself.
//!
//! `gst`: pulls in GStreamer. It unlocks the `unixfd` and `shm` transports on
//! Unix, and a `matroskamux` backed container writer. On Windows the fd
//! transports are compiled out and asking for one returns an error that names
//! the container transport as the way forward.
//!
//! # Where the rules come from
//!
//! The manifest, the media contract and the protocol are specified in
//! `docs/reference/plugin-manifest.md` and `docs/reference/plugin-protocol.md`
//! in this repository. Where this crate and those pages disagree, the pages are
//! right and this crate has a bug.

#![forbid(unsafe_code)]

pub mod crash;
pub mod driver;
pub mod env;
pub mod framing;
pub mod handshake;
pub mod manifest;
pub mod media;
pub mod pacing;
pub mod plugin;
pub mod runtime;
pub mod skill;
pub mod transcript;
pub mod wire;

/// The api level this crate implements.
pub const API_LEVEL: u32 = 1;

/// Everything a plugin author usually wants, in one `use`.
pub mod prelude {
    pub use crate::driver::VideoLoop;
    pub use crate::env::PluginEnv;
    pub use crate::manifest::Manifest;
    pub use crate::media;
    pub use crate::pacing::{AudioPacer, FramePool, Pacer};
    pub use crate::plugin::{
        Device, DeviceHandler, Filter, FilterHandler, Output, OutputHandler, Reporter, Service,
        ServiceHandler, Source, SourceHandler, Transition, TransitionHandler,
    };
    pub use crate::runtime;
    pub use crate::wire::{
        codes, AudioSet, AudioState, Candidate, Canvas, Configure, Health, HealthState,
        InitializeResult, LogLevel, Position, Ready, RpcError, StartParams, StartResult, Transport,
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_api_level_matches_the_manifest_the_templates_ship() {
        assert_eq!(super::API_LEVEL, 1);
    }
}
