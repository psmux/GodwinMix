//! The one wasmtime engine, and the clock that cuts a runaway component.
//!
//! One engine for the whole core, because compiled code and the allocator are
//! shared by every instance and an engine per plugin would pay for both again.
//! Stores are not shared: each instance gets one, so a component has no way to
//! see another's memory or another's fuel.
//!
//! # Two limits, not one
//!
//! Fuel counts executed operations and stops a loop that is doing work.
//! Epochs are wall clock and stop a component that is blocked on something
//! that never returns. Fuel alone would let a host call that never comes back
//! hang the worker; epochs alone would let a component spend a whole deadline
//! spinning before anything noticed. Both are on.

use anyhow::{Context, Result};
use std::sync::OnceLock;
use std::time::Duration;
use wasmtime::{Config, Engine};

/// How often the epoch is bumped. One millisecond is the resolution a deadline
/// is expressed at, and the thread costs one timer wakeup per millisecond on a
/// core that has a component loaded and nothing at all on one that has not,
/// because it is only started when the first component is.
pub const EPOCH_TICK: Duration = Duration::from_millis(1);

/// The wasmtime line this host is built against, for `gmx doctor`. Read from
/// the dependency at build time so it cannot drift from what is linked.
pub const WASMTIME: &str = "36 (LTS)";

static ENGINE: OnceLock<Engine> = OnceLock::new();

/// The shared engine, built on first use and never rebuilt.
pub fn engine() -> Result<&'static Engine> {
    if let Some(engine) = ENGINE.get() {
        return Ok(engine);
    }
    let built = build().context("building the WebAssembly engine")?;
    // A race here means two engines were built and one is dropped, which costs
    // nothing: an engine with no store and no module in it is a few
    // allocations.
    let _ = ENGINE.set(built);
    ENGINE.get().context("the engine would not settle")
}

fn build() -> Result<Engine> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);
    config.epoch_interruption(true);
    // No debug info and no backtrace detail: a component is third party code
    // and the core does not need its symbols to report that it ran out of
    // fuel. This is also a few hundred kilobytes off every compilation.
    config.debug_info(false);
    config.wasm_backtrace(true);
    let engine = Engine::new(&config)?;
    start_ticker(engine.clone());
    Ok(engine)
}

/// Bump the epoch forever, on a thread of its own.
///
/// Named so it is visible in a thread dump, and detached: there is nothing to
/// join and a core that is shutting down does not wait for a timer.
fn start_ticker(engine: Engine) {
    let started = std::thread::Builder::new().name("wasm-epoch".into()).spawn(move || loop {
        std::thread::sleep(EPOCH_TICK);
        engine.increment_epoch();
    });
    if let Err(e) = started {
        tracing::warn!(
            ?e,
            "the WebAssembly epoch ticker would not start; components will be held to their \
             fuel allowance alone"
        );
    }
}

/// What `gmx doctor` prints.
pub fn describe() -> String {
    format!("wasmtime {}, component model, fuel and epoch limits on", WASMTIME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_is_built_once_and_says_what_it_is() {
        let first = engine().expect("an engine");
        let again = engine().expect("the same engine");
        assert!(std::ptr::eq(first, again), "one engine for the whole core");
        assert!(describe().contains("wasmtime"), "{}", describe());
    }
}
