//! The tier 2 plugin host: nothing yet.
//!
//! A tier 2 plugin is a separate process that the core starts and supervises.
//! It speaks the same JSON-RPC control protocol as every other client, over a
//! pipe rather than a socket, and it hands media across the process boundary
//! through a transport the two agree on at handshake. A crash costs one
//! source and never the programme output.
//!
//! What lands here, in the order 03 section 5 builds it:
//!
//! * `manifest`: reading and validating `gmx-plugin.toml`.
//! * `handshake`: the version and capability exchange that picks a transport.
//! * `transport`: unixfd on Linux and macOS, a container on a pipe everywhere.
//! * `loader`: start, supervise, restart, budget and kill.
//!
//! It is a crate of its own rather than a module of `godwinmix-core` because
//! the engine must be embeddable without a plugin loader linked in, and
//! because a plugin author's own host process wants this without the engine.
//!
//! See `README.md` beside this file.

/// What the host will answer when it is asked what it can do.
///
/// A placeholder with a real meaning: the tier this crate implements. It is
/// here so the crate has a target, compiles, and is a workspace member from
/// the day the layout lands rather than the day the loader does.
pub const TIER: u8 = 2;

/// The protocol level this host speaks, which is the core's.
pub fn api_level() -> u32 {
    godwinmix_protocol::API_LEVEL
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_host_speaks_the_same_protocol_level_as_the_core() {
        assert_eq!(super::api_level(), godwinmix_protocol::API_LEVEL);
        assert_eq!(super::TIER, 2);
    }
}
