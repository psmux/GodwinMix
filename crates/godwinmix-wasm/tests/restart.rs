//! A component that traps costs its instance and not the plugin.
//!
//! wasmtime marks a trapped component instance unusable: every later call
//! answers "cannot enter component instance", which tells an operator nothing
//! and would leave a transition silently broken for the rest of a show. So the
//! supervisor treats a trap the way it treats a process that died: the store
//! is dropped, a fresh one is built from the same file on the next pump pass,
//! and the next call works.
//!
//! No mixer here, because none is needed. This is the supervisor's own
//! behaviour and it is worth testing without a pipeline in the way.

use godwinmix_core::plugin::supervisor::Supervisor;
use godwinmix_core::plugin::wasm;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

fn hold() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/min-hold")
}

/// `wasm_fuel = 1000`: enough to enter an export and not enough to leave it.
fn starved() -> BTreeMap<String, toml::Table> {
    let table: toml::Table = toml::from_str("wasm_fuel = 1000").expect("a table");
    BTreeMap::from([("min-hold".to_string(), table)])
}

#[test]
fn a_component_that_trapped_is_built_again_and_answers_the_next_call() {
    godwinmix_wasm::install();
    let dir = std::env::temp_dir().join(format!("gmx-wasm-restart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a plugins directory");
    godwinmix_core::plugin::loader::set_dir(dir.clone());
    godwinmix_core::plugin::loader::set_runtime_dir(dir.join("run"));
    godwinmix_core::plugin::loader::install_from_path(&hold()).expect("installing min-hold");

    let supervisor = Supervisor::new(
        godwinmix_core::caps::CanvasCaps::new(&Default::default()),
        starved(),
    );
    let failures = supervisor.start_all();
    assert!(failures.is_empty(), "the component should load: {failures:?}");

    let live = wasm::live("min-hold").expect("the component is in the live table");
    assert_eq!(live.hooks(), ["take.before", "take.after"]);

    // One call, which runs out of fuel and spends the instance.
    let hook = json!({"hook": "take.before", "payload": {}});
    let e = live.call("hook", hook.clone()).expect_err("a thousand operations is not enough");
    assert!(format!("{e:#}").contains("fuel") || format!("{e:#}").contains("trap"), "{e:#}");
    assert!(
        live.call("health", json!({})).is_err(),
        "a trapped instance cannot be entered again, which is why it has to be replaced"
    );

    // Give it enough fuel to finish a call, so the rebuilt instance can be
    // asked something and answer. This also says that a settings change takes
    // effect on the next build, which is what `plugin.reload` relies on.
    supervisor.set_settings(BTreeMap::new());

    // One pump pass, which is what a running core does every 250 ms.
    supervisor.pump();

    let fresh = wasm::live("min-hold").expect("a fresh component took its place");
    assert!(
        !std::sync::Arc::ptr_eq(&live, &fresh),
        "the live table should hold the new instance, not the spent one"
    );
    let answer = fresh
        .call_within("hook", hook, Duration::from_millis(500))
        .expect("the fresh instance answers");
    assert_eq!(
        answer["allow"],
        json!(true),
        "the fresh instance starts with no take behind it, so the first one goes through: \
         {answer}"
    );

    supervisor.shutdown();
    let _ = godwinmix_core::plugin::loader::uninstall("min-hold");
    let _ = std::fs::remove_dir_all(dir);
}
