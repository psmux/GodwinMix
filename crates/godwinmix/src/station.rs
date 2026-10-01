//! The station: the process a person starts, when shows run as processes.
//!
//! ```text
//!   browser, CLI, agent ──► control port (the station)
//!                             ├─ show.*, channel.*, governor.*: answered here
//!                             └─ everything else: relayed to one show's socket
//!                                   ├─ show main   (its own process)
//!                                   └─ show b      (its own process)
//!   each show ──link──► station: governor.admit, on air, hello
//! ```
//!
//! The station owns the control port, the page, the ingest hub (the ingest
//! plugin, run once), the governor and the supervisor. It mixes nothing. A
//! show is the same binary run with `--show <id> --station <link>`, bound to
//! a loopback port it picks and tells the station over the link; nobody
//! else ever sees that port. See `dev/plans/wave3-contract.md` and
//! `docs/explanation/architecture.md`.

pub mod link;
pub mod registry;
mod ask;
mod channel_calls;
pub mod direct;
pub mod child;
mod files;
mod host;
mod ingest;
pub mod list;
pub mod methods;
mod programme;
mod project;
pub mod relay;
mod run;
mod server;
pub mod show;
mod shows_api;
mod shows_call;
mod shows_bulk;
mod shows_direct;
mod shows_set;
mod shows_stats;
mod switch;
mod thumb;
pub mod usage;
pub mod state;
pub mod supervise;

pub use run::{run, Options};
