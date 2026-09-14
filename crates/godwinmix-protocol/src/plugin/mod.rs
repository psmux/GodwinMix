//! The plugin contract: `gmx-plugin.toml`, the handshake, and the bodies of
//! the methods a plugin implements.
//!
//! Both sides read these types. The core's tier 2 host parses a manifest with
//! `manifest`, speaks `wire` over the pipe, and replays a `transcript` when a
//! plugin is tested with no core at all. A plugin written in Rust uses the
//! same types through `godwinmix-sdk`, which re-exports this module rather
//! than carrying a second copy of it: one description of a protocol cannot
//! drift, two always do.
//!
//! ```text
//!   manifest.rs    gmx-plugin.toml: types, parser, validator with key paths
//!   wire.rs        the handshake and every method body that crosses the pipe
//!   skill.rs       SKILL.md frontmatter, and the check the harness makes
//!   transcript.rs  the recorded transcript `gmx plugin test --offline` replays
//! ```
//!
//! Nothing here spawns a process or opens a socket. The manifest validator
//! reads files only when it is handed a root to check paths against, and it
//! runs on a machine with no GStreamer.

pub mod manifest;
pub mod skill;
pub mod transcript;
pub mod wire;

pub use manifest::{Manifest, ManifestError, Problem, Provide};
pub use wire::{Canvas, Frame, Transport};
