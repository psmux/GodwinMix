//! `gmx-tally`: TSL UMD v5 tally for GodwinMix, as a `service` plugin.
//!
//! Tally lamps do not speak JSON. What they speak is TSL, and this plugin is
//! the translation: it subscribes to `event/tally` on the core's `/rpc` and
//! turns every change into a TSL UMD v5.0 packet on UDP or TCP.
//!
//! It also runs by hand, which is how it is tested against a live core while
//! the mixer does not yet instantiate `service` plugins itself:
//!
//! ```sh
//! gmx-tally --url http://127.0.0.1:8080 --token TOKEN \
//!           --to 10.0.0.30:8900 --lamp cam1:0:"CAM 1" --lamp cam2:1:"CAM 2"
//! ```
//!
//! | Module | What it is |
//! |---|---|
//! | [`tsl`] | the protocol on the wire, encoder and decoder |
//! | [`settings`] | `schemas/tally.json`, read into a struct |
//! | [`sender`] | UDP and TCP, with the reconnect TCP needs |
//! | [`service`] | the board of what each lamp is showing, and the loop |

pub mod sender;
pub mod service;
pub mod settings;
pub mod tsl;

pub use settings::{Lampinfo, Protocol, Settings};
pub use tsl::{Display, Lamp};
