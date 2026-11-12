//! `gmx-atem`: Blackmagic ATEM switchers for GodwinMix, as a `service`
//! plugin.
//!
//! The protocol is `godwinmix-atem`'s; this is what joins it to the mixer
//! and to agents: several switchers under slugs, every bus and keyer as a
//! tool, and the follow and lead modes that make GodwinMix and a switcher
//! cut together.
//!
//! | Module | What it is |
//! |---|---|
//! | [`settings`] | `schemas/atem.json`, read into structs |
//! | [`desk`] | every switcher, with its name, mode and source map |
//! | [`view`] | what `list`, `state` and `tally` answer |
//! | [`tools`] | every tool |
//! | [`sync`] | follow and lead, and the guard against loops |
//! | [`service`] | the plugin protocol's `Service`, and the command line mode |

pub mod desk;
pub mod service;
pub mod settings;
pub mod sync;
pub mod tools;
pub mod view;

pub use settings::{Mode, Settings, SwitcherSpec};

/// Where log lines go: the core's log in a sidecar, stderr by hand.
pub trait Log: Send + Sync + 'static {
    fn info(&self, message: &str);
    fn warn(&self, message: &str);
}

pub struct Stderr;

impl Log for Stderr {
    fn info(&self, message: &str) {
        eprintln!("gmx-atem: {message}");
    }
    fn warn(&self, message: &str) {
        eprintln!("gmx-atem: {message}");
    }
}
