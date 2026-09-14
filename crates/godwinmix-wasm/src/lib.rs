//! The tier W host: a GodwinMix plugin as a WebAssembly component.
//!
//! The core holds the shape of a component instance and a registry with room
//! for one runner (`godwinmix_core::plugin::wasm`); this crate is the runner.
//! The split is the reason the engine does not carry wasmtime: a build without
//! `--features wasm` never links this crate at all, and the core's registry is
//! simply empty.
//!
//! ```text
//!   binary (--features wasm)
//!     godwinmix_wasm::install()
//!            |
//!            v
//!   godwinmix_core::plugin::wasm::register(Runner)
//!            |
//!            v  supervisor: a service or transition with placements = ["wasm"]
//!   Component::start(spec) -> a worker thread with one wasmtime Store
//! ```
//!
//! Nothing here is on the frame path. `render` is answered off the mixer
//! thread and sampled before the transition window opens, so a component that
//! runs out of fuel costs a take its curve and never costs the programme a
//! frame. `docs/explanation/why-wasm-is-not-on-the-frame-path.md` is the long
//! version.

pub mod bindings;
pub mod engine;
pub mod hostcalls;
pub mod instance;
pub mod worker;

use godwinmix_core::plugin::wasm::{self, Instance, Runner, Spec};
use std::sync::Arc;

/// The runner this crate registers.
pub struct Wasmtime;

impl Runner for Wasmtime {
    fn start(&self, spec: Spec) -> anyhow::Result<Arc<dyn Instance>> {
        instance::Component::start(spec)
    }

    fn describe(&self) -> String {
        engine::describe()
    }
}

/// Register the runner with the core. Called once, by the binary.
pub fn install() {
    wasm::register(Arc::new(Wasmtime));
}
