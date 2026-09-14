//! `gmx-osc`: an OSC bridge for GodwinMix, as a `service` plugin.
//!
//! A service plugin carries no media. It is a process the core starts, it
//! talks the same JSON lines protocol as every other plugin on stdin and
//! stderr, and everything it actually does it does over the core's `/rpc`,
//! whose URL arrives in `GMX_RPC` and whose token arrives in `GMX_TOKEN`.
//! That is the whole shape, and it is why this plugin also runs by hand:
//!
//! ```sh
//! gmx-osc --url http://127.0.0.1:8080 --token TOKEN --listen 9000
//! ```
//!
//! # What is in here
//!
//! | Module | What it is |
//! |---|---|
//! | [`osc`] | OSC 1.0 on the wire, encoder and decoder, about 300 lines |
//! | [`map`] | an address becomes one call on the core, or a refusal that says why |
//! | [`settings`] | `schemas/service.json`, read into a struct |
//! | [`service`] | the socket, the event stream, and the loop between them |
//!
//! Nothing in [`osc`] or [`map`] touches a socket, so the behaviour of this
//! plugin is tested without one.

pub mod map;
pub mod osc;
pub mod service;
pub mod settings;

pub use map::{Action, Refusal};
pub use osc::{Arg, Message};
pub use settings::Settings;
