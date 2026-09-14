//! `godwinmix-tui`: the reference terminal UI, and a plain client of `/rpc`.
//!
//! It holds no private channel to the core. Every number on the screen arrives
//! as an event any client may subscribe to, and every key sends a JSON-RPC
//! method any client may call. That is the point of keeping it first party:
//! the protocol stays honest because the terminal UI would break first.
//!
//! The library half is here so the tests can drive the whole surface (the
//! store, the key map, the calls) against a fake core with no terminal
//! anywhere near them.

pub mod app;
pub mod args;
pub mod client;
pub mod keys;
pub mod model;
pub mod picture;
pub mod store;
pub mod term;
pub mod ui;
