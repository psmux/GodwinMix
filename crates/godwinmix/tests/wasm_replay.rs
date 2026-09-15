//! Phase 6's tier W acceptance: the same recorded session replayed with and
//! without a WebAssembly service plugin, and the difference in what happened.
//!
//! `tests/sessions/min-hold-wasm.jsonl` is three takes, two of them two
//! seconds apart. Replayed against a bare test core, every take lands: the
//! core's own `[safety] min_hold_ms` is off for a replay, because holding a
//! replay to it would refuse takes the original run made.
//!
//! Replayed with `plugins/min-hold` installed, the take that arrives two
//! seconds after the last one is refused by a `take.before` hook answering
//! `{allow: false, reason}` from inside a component. The programme therefore
//! has one fewer change in it, and that difference is the measurement.
//!
//! Needs the `wasm` feature, because without it there is no host to run the
//! component in and the plugin would not start.

#![cfg(feature = "wasm")]

use godwinmix::cli::session::{self, Options};
use std::path::{Path, PathBuf};

fn log() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/sessions/min-hold-wasm.jsonl")
}

fn expectations(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/sessions").join(name)
}

fn plugin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/min-hold")
}

/// Every take the recording made, made again.
async fn replay(expect: &Path, with: Vec<PathBuf>) -> session::Outcome {
    // The runner is registered by `godwinmix::run` on a real core. A test
    // core is built without going through it, so it registers its own.
    godwinmix::install_wasm_host();
    let options = Options {
        source_fixture: None,
        // The recorded gaps are the whole point here: a minimum hold is a
        // rule about time, and replaying as fast as the commands will go
        // would make every take fall inside any window at all.
        fast: false,
        expect: Some(expect.to_path_buf()),
        write_expectations: false,
        with_plugins: with,
    };
    session::replay(&log(), &options).await.expect("the replay runs")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_session_replays_differently_with_the_min_hold_component() {
    let without = replay(&expectations("min-hold-wasm.expect_changes.json"), Vec::new()).await;
    assert!(
        without.passed(),
        "with no plugin the recording should replay exactly: {:?}",
        without.differences
    );

    let with = replay(
        &expectations("min-hold-wasm.with-plugin.expect_changes.json"),
        vec![plugin()],
    )
    .await;
    assert!(
        with.passed(),
        "with the plugin the replay should match its own expectations: {:?}",
        with.differences
    );

    // The difference, stated rather than implied: one take fewer reached the
    // programme, and the core said why.
    let takes = |o: &session::Outcome| {
        o.produced.iter().filter(|d| d.what == "program").count()
    };
    assert_eq!(takes(&without), 3, "every take lands with no plugin holding the shot");
    assert_eq!(takes(&with), 2, "the take inside eight seconds is refused");

    let refusal = with
        .errors
        .iter()
        .find(|e| e.contains("-32003"))
        .unwrap_or_else(|| panic!("the refusal should be in the report: {:?}", with.errors));
    assert!(refusal.contains("min-hold"), "the refusal names the hook that made it: {refusal}");
    assert!(
        refusal.contains("8000 ms"),
        "and the window it is enforcing, so an operator can act on it: {refusal}"
    );
    assert!(
        without.errors.iter().all(|e| !e.contains("-32003")),
        "nothing refuses a take without the plugin: {:?}",
        without.errors
    );
}
