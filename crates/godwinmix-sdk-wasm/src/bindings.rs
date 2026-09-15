//! The generated guest bindings.
//!
//! One `generate!`, for the world that exports both interfaces. A component
//! built with this crate therefore exports `service` and `transition` whether
//! it implements one or both, and the half it does not implement answers
//! -32601. The host instantiates it under the narrow world its provide asked
//! for and never calls the other half, so the cost is a few hundred bytes of
//! stub and the gain is one set of bindings, one `cabi_realloc`, and one
//! export macro for an author to remember.
#![allow(clippy::all)]

wit_bindgen::generate!({
    world: "plugin",
    path: "../../wit",
    pub_export_macro: true,
    export_macro_name: "export_plugin",
    default_bindings_module: "godwinmix_sdk_wasm::bindings",
});
