//! `godwinmix-atem`: a client for Blackmagic ATEM switchers.
//!
//! The switcher speaks a UDP protocol on port 9910 that Blackmagic has not
//! published. `docs/reference/atem-protocol.md` is this project's own
//! description of it, written from the community's notes.
//!
//! # Layers
//!
//! | Module | What it is |
//! |---|---|
//! | [`wire`] | the packet header and command blocks, bytes only |
//! | [`command`] | every command this client sends, with its exact bytes |
//! | [`state`] | the switcher's state, folded in one block at a time |
//! | [`session`] | the handshake, acks, resends and timeouts, with no socket |
//! | [`client`] | a tokio task that drives one session over a real socket |
//! | [`registry`] | several switchers at once, each under a slug |
//!
//! Everything below [`client`] is free of IO and of the clock, so the rules
//! of the protocol are tested without a network.
//!
//! ```no_run
//! # async fn demo() -> Result<(), godwinmix_atem::Error> {
//! use godwinmix_atem::{Command, Switchers};
//! let mut switchers = Switchers::default();
//! let atem = switchers.add("atem-main", "192.0.2.10")?;
//! atem.wait_up(std::time::Duration::from_secs(5)).await?;
//! atem.send(vec![Command::Preview { me: 0, source: 2 }, Command::Auto { me: 0 }]).await?;
//! # Ok(()) }
//! ```

pub mod client;
pub mod command;
mod error;
pub mod registry;
pub mod session;
pub mod source;
pub mod state;
pub mod wire;

pub use client::{Atem, Snapshot};
pub use command::{Command, RateStyle, TransitionStyle};
pub use error::Error;
pub use registry::Switchers;
pub use session::{Link, LinkStatus, Session};
pub use state::State;

/// The port every ATEM listens on.
pub const PORT: u16 = 9910;
