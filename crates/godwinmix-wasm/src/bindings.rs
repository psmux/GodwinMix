//! The generated bindings, one module per world.
//!
//! Two narrow worlds rather than one wide one. A component that provides a
//! service exports `service` and nothing else, and a world that also demanded
//! `transition` would refuse to instantiate it. A component that exports both
//! satisfies either world, because wasmtime looks up the exports a world names
//! and ignores the rest, so the author with one component and two provides is
//! not made to split it.
//!
//! The `with` mapping on the second world points its `host` and `types` at the
//! first world's generated modules, so there is one `Host` trait to implement
//! and one set of records, not two of each.

pub mod service {
    wasmtime::component::bindgen!({
        world: "service-plugin",
        path: "../../wit",
    });
}

pub mod transition {
    wasmtime::component::bindgen!({
        world: "transition-plugin",
        path: "../../wit",
        with: {
            "godwinmix:plugin/host": crate::bindings::service::godwinmix::plugin::host,
            "godwinmix:plugin/types": crate::bindings::service::godwinmix::plugin::types,
        },
    });
}

pub use service::godwinmix::plugin::host;
pub use service::godwinmix::plugin::types;
