//! The subcommands that are not `ctl`: scenes, codecs, observability.
//!
//! Each is a `clap::Subcommand` plus the code that carries it out. They sit
//! here rather than beside the engine modules they drive, because the engine
//! is a library that must be embeddable without a command line parser linked
//! in. `crate::ctl`, `crate::mcp` and `crate::bench` are the same idea and are
//! large enough to be files of their own.

pub mod bundle;
pub mod chaos;
pub mod codec;
pub mod observe;
pub mod plugin;
pub mod scene;
