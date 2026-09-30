//! The resource governor: one budget for the whole machine.
//!
//! Before anything that costs CPU, a hardware encoder session, memory or
//! uplink starts, it asks [`Governor::admit`]. The answer is a [`Ticket`]
//! that holds the share until it is dropped, or a refusal that says what the
//! work needs, what is left, and what would fit instead. Nothing is started
//! that would take frames from what is already on air.
//!
//! What is left is worked out here and never typed by a person
//! (`dev/plans/shows-and-renditions.md`, Decision 2): the machine is measured
//! once by [`calibrate`] (behind the `calibrate` feature, the only part that
//! touches GStreamer), its live load is sampled once a second by
//! [`load::Sampler`], and [`headroom`] turns the two into a number with a
//! reserve sized for the machine it is on.
//!
//! When the machine runs short while on air, [`Governor::shed`] says what to
//! drop first. The governor decides the order; the caller does the dropping.
//!
//! See `docs/explanation/resource-governor.md` for why it works this way and
//! `docs/reference/governor.md` for the surface.

pub mod advice;
pub mod calibration;
pub mod config;
pub mod fingerprint;
mod fit;
mod governor;
pub mod headroom;
pub mod load;
pub mod profile;
pub mod shed;
pub mod store;
mod ticket;

#[cfg(test)]
mod testing;

#[cfg(feature = "calibrate")]
pub mod calibrate;

pub use advice::Advice;
pub use calibration::Calibration;
pub use config::GovernorConfig;
pub use governor::{Admit, Claim, Governor};
pub use profile::Profile;
pub use shed::{Kind, ShedAction, ShedStep};
pub use ticket::Ticket;
