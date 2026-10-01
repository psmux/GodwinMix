//! The receive and send halves of `udp/source` and `udp/output`, as a library.
//!
//! The binary in `main.rs` is the plugin. This library is what it is built
//! from, made a library so the direct host in `plugins/ingest` can take a
//! multicast feed with the same parsing, program choice and loss counting,
//! and the same tests behind them, instead of a second copy.

pub mod address;
pub mod counters;
pub mod iface;
pub mod recv;
pub mod send;
pub mod ts;
