//! `gmx-plugin.toml`: re-exported from `godwinmix-protocol`.
//!
//! The manifest types, the vocabularies and the validator live in
//! `godwinmix_protocol::plugin::manifest`, because the core reads a manifest
//! with exactly the code a plugin author writes one against. Two copies of a
//! schema drift; one cannot. Everything that was here is still reachable under
//! the same names, so `use godwinmix_sdk::manifest::Manifest;` is unchanged.

pub use godwinmix_protocol::plugin::manifest::*;
