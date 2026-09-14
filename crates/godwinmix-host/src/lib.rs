//! The tier 2 plugin host: everything about running a plugin beside the core
//! that is not a pipeline.
//!
//! A tier 2 plugin is a separate process. It speaks the same JSON-RPC control
//! protocol as every other client, over a pipe rather than a socket, and it
//! hands media across the process boundary through a transport the two agree
//! on at the handshake. A crash costs one source and never the programme
//! output.
//!
//! ```text
//!   launch.rs     [run] and [build] turned into argv, an environment and a cwd
//!   channel.rs    JSON lines with the 4 MiB limit, and ids in flight per direction
//!   handshake.rs  the transport negotiation and the api range check
//!   lifecycle.rs  the state machine of 03 section 7 and the restart backoff
//!   budget.rs     [plugins.<name>] limits, and what happens on a breach
//!   sampler.rs    cpu and rss per process, read cheaply once a second
//!   offline.rs    `gmx plugin test --offline`: a transcript, a binary, no core
//!   probe.rs      does a new build still say hello? what an update rolls back on
//!   sources/      where a plugin comes from: github, git, cargo, npm, pypi, oci, path
//!   verify.rs     the sigstore bundle check and the api range check
//!   marketplace.rs  the JSON listing a plugin is resolved through
//! ```
//!
//! It is a crate of its own rather than a module of `godwinmix-core` because
//! the engine must be embeddable without a plugin loader linked in, and
//! because a plugin author's own host process wants this without the engine.
//! The one thing it does not own is the child process itself: the core already
//! has the process group teardown, the orphan reaper and the Windows stdout
//! reader that a sidecar needs, and a second copy of those would be a second
//! set of bugs.
//!
//! See `README.md` beside this file.

pub mod budget;
pub mod channel;
pub mod handshake;
pub mod launch;
pub mod lifecycle;
pub mod marketplace;
pub mod offline;
pub mod probe;
pub mod sampler;
pub mod sources;
pub mod verify;

pub use budget::{Budget, OverBudget, Stats};
pub use channel::{Channel, LineError, Pending};
pub use handshake::{negotiate, Negotiated, HANDSHAKE_TIMEOUT};
pub use launch::{Launch, LaunchCtx, Runtime};
pub use lifecycle::{Backoff, Lifecycle};
pub use marketplace::{Listing, Marketplace, Tier};
pub use sources::{FetchCtx, Fetched, Source};
pub use verify::{Level, Signature, Trust};

/// The tier this crate implements.
pub const TIER: u8 = 2;

/// The protocol level this host speaks, which is the core's.
pub fn api_level() -> u32 {
    godwinmix_protocol::API_LEVEL
}

/// How long a plugin has to answer `shutdown` before its process group is
/// killed. 03 section 7: stop, then shutdown, then the group after 8 seconds.
pub const SHUTDOWN_GRACE_SECS: u64 = 8;

#[cfg(test)]
mod tests {
    #[test]
    fn the_host_speaks_the_same_protocol_level_as_the_core() {
        assert_eq!(super::api_level(), godwinmix_protocol::API_LEVEL);
        assert_eq!(super::TIER, 2);
    }
}
