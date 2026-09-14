//! The two components in the tree, driven through the host.
//!
//! These run against the committed `plugin.wasm` files rather than building
//! them, so a checkout with no wasm toolchain still runs them.
//! `dev/build-wasm.sh` rebuilds the components from source.

use godwinmix_core::caps::CanvasCaps;
use godwinmix_core::plugin::wasm::{Grant, Spec};
use godwinmix_core::plugin::ProvideKind;
use godwinmix_wasm::instance::Component;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn spec(plugin: &str, provide: &str, kind: ProvideKind, at: &str, params: Value) -> Spec {
    let root = repo().join(at);
    Spec {
        plugin: plugin.to_string(),
        provide: provide.to_string(),
        instance: format!("{plugin}-{provide}"),
        kind,
        component: root.join("plugin.wasm"),
        root,
        canvas: CanvasCaps::new(&Default::default()),
        params,
        grant: Grant::default(),
    }
}

fn hold(window_ms: u64) -> std::sync::Arc<dyn godwinmix_core::plugin::wasm::Instance> {
    Component::start(spec(
        "min-hold",
        "hold",
        ProvideKind::Service,
        "plugins/min-hold",
        json!({ "min_hold_ms": window_ms }),
    ))
    .expect("the min-hold component loads")
}

fn take_before() -> Value {
    json!({"hook": "take.before", "ts": "2026-09-15T00:00:00Z", "payload": {"source": "cam2"}})
}

#[test]
fn the_first_take_is_allowed_and_the_second_inside_the_window_is_refused() {
    let hold = hold(8_000);
    let first = hold.call("hook", take_before()).expect("the hook answers");
    assert_eq!(first["allow"], json!(true), "the first take has nothing to hold it: {first}");

    let second = hold.call("hook", take_before()).expect("the hook answers again");
    assert_eq!(second["allow"], json!(false), "a take inside the window is refused: {second}");
    let reason = second["reason"].as_str().expect("a refusal carries a reason");
    assert!(reason.contains("8000 ms"), "the reason names the window: {reason}");
    assert!(reason.contains("Wait"), "the reason names the next step: {reason}");
    assert!(
        reason.contains("min_hold_ms"),
        "and the setting that changes it, so an operator can act on it: {reason}"
    );
}

#[test]
fn a_window_of_nothing_refuses_nothing() {
    let hold = hold(0);
    for _ in 0..3 {
        let answer = hold.call("hook", take_before()).expect("the hook answers");
        assert_eq!(answer["allow"], json!(true), "a window of 0 holds nothing: {answer}");
    }
}

#[test]
fn a_hook_the_plugin_does_not_want_is_answered_rather_than_refused() {
    let hold = hold(8_000);
    let answer = hold
        .call("hook", json!({"hook": "session.start", "payload": {}}))
        .expect("an unwanted hook still answers");
    assert_ne!(answer["allow"], json!(false), "only take.before may refuse: {answer}");
}

#[test]
fn the_tool_says_how_long_is_left_and_the_handshake_declared_it() {
    let hold = hold(8_000);
    assert_eq!(hold.tools(), ["hold_state"], "the handshake named the tool");
    assert_eq!(hold.hooks(), ["take.before", "take.after"], "and the hooks");

    let before = hold
        .call("tool.call", json!({"name": "hold_state", "arguments": {}}))
        .expect("the tool answers");
    assert_eq!(before["structuredContent"]["remaining_ms"], json!(0), "{before}");

    hold.call("hook", take_before()).expect("a take");
    let after = hold
        .call("tool.call", json!({"name": "hold_state", "arguments": {}}))
        .expect("the tool answers");
    let left = after["structuredContent"]["remaining_ms"].as_u64().expect("a number");
    assert!(left > 7_000 && left <= 8_000, "nearly the whole window is left: {left}");
}

#[test]
fn health_and_configure_answer_the_way_the_protocol_says() {
    let hold = hold(8_000);
    let health = hold.call("health", json!({})).expect("health answers");
    assert_eq!(health["state"], json!("ok"), "{health}");

    let applied = hold
        .call("configure", json!({"params": {"min_hold_ms": 2000}}))
        .expect("configure answers");
    assert_eq!(applied["applied"], json!(true), "{applied}");
}

#[test]
fn a_component_that_is_asked_for_a_method_it_has_no_export_for_says_so() {
    let hold = hold(8_000);
    let e = hold.call("render", json!({})).expect_err("a service has no render");
    assert!(format!("{e:#}").contains("render"), "{e:#}");
}

#[test]
fn an_ease_answers_render_once_with_a_curve() {
    let ease = Component::start(spec(
        "wasm-ease",
        "ease",
        ProvideKind::Transition,
        "examples/wasm-ease",
        Value::Null,
    ))
    .expect("the wasm-ease component loads");

    let request = json!({
        "from": ["sink_0"],
        "to": ["sink_1"],
        "progress": 0.0,
        "running_time_ns": 0u64,
    });
    let answer = ease.call_within("render", request, Duration::from_millis(50)).expect("a curve");
    let points = answer["curve"].as_array().expect("a curve is an array of points");
    assert!(points.len() > 8, "a curve needs points to be a curve: {}", points.len());
    assert_eq!(points[0], json!([0.0, 0.0]), "it starts at nothing");
    assert_eq!(points[points.len() - 1], json!([1.0, 1.0]), "and ends at everything");
    let half = points[points.len() / 2][1].as_f64().expect("a number");
    assert!((half - 0.5).abs() < 0.05, "and passes through the middle: {half}");
}

#[test]
fn a_transition_with_no_pads_on_either_side_refuses_rather_than_driving_nothing() {
    let ease = Component::start(spec(
        "wasm-ease",
        "ease",
        ProvideKind::Transition,
        "examples/wasm-ease",
        Value::Null,
    ))
    .expect("the component loads");
    let e = ease
        .call("render", json!({"from": [], "to": [], "progress": 0.0}))
        .expect_err("nothing to ease");
    assert!(format!("{e:#}").contains("nothing to ease"), "{e:#}");
}

#[test]
fn a_call_that_runs_out_of_fuel_is_cut_and_says_so() {
    // A thousand operations is enough to enter the export and not enough to
    // come back out of it, so this measures the cut rather than the work.
    let mut spec = spec(
        "min-hold",
        "hold",
        ProvideKind::Service,
        "plugins/min-hold",
        json!({ "min_hold_ms": 8000 }),
    );
    spec.grant.fuel_per_call = 1_000;
    let hold = Component::start(spec).expect("loading spends no fuel the call needs");
    let e = hold.call("hook", take_before()).expect_err("a thousand operations is not enough");
    let said = format!("{e:#}");
    assert!(
        said.contains("fuel") || said.contains("trap") || said.contains("hook"),
        "the error has to say which call was cut: {said}"
    );
}

#[test]
fn a_component_that_has_been_shut_down_answers_rather_than_hanging() {
    let hold = hold(8_000);
    hold.shutdown("the test is over");
    let e = hold.call("health", json!({})).expect_err("there is nothing left to ask");
    assert!(format!("{e:#}").contains("shut down"), "{e:#}");
}

#[test]
fn a_component_file_that_is_not_one_is_refused_with_the_build_line() {
    let mut spec = spec(
        "min-hold",
        "hold",
        ProvideKind::Service,
        "plugins/min-hold",
        Value::Null,
    );
    spec.component = repo().join("README.md");
    let Err(e) = Component::start(spec) else {
        panic!("a markdown file is not a component");
    };
    let said = format!("{e:#}");
    assert!(said.contains("wasm32-wasip2"), "the message must name the build target: {said}");
}

#[test]
fn a_component_that_trapped_says_what_it_can_do_next_rather_than_answering_oddly() {
    // What happens to the store after a trap decides how the supervisor has to
    // treat one. If the next call works, a cut is a cut and nothing else needs
    // doing; if it does not, the instance is spent and has to be said to be.
    let mut spec = spec(
        "min-hold",
        "hold",
        ProvideKind::Service,
        "plugins/min-hold",
        json!({ "min_hold_ms": 8000 }),
    );
    spec.grant.fuel_per_call = 1_000;
    let hold = Component::start(spec).expect("it loads");
    let first = hold.call("hook", take_before()).expect_err("out of fuel");
    let second = hold.call("health", json!({}));
    println!("after a trap: {second:?}");
    assert!(
        format!("{first:#}").contains("fuel") || format!("{first:#}").contains("trap"),
        "{first:#}"
    );
    // wasmtime marks a trapped component instance unusable, so the honest
    // answer is that the instance is spent and something will build another.
    // Anything else here would mean an operator reading "cannot enter
    // component instance" for the rest of the show.
    let Err(e) = second else { panic!("a trapped component cannot be entered again") };
    let said = format!("{e:#}");
    assert!(said.contains("trapped"), "the error must say what happened: {said}");
    assert!(said.contains("fresh one"), "and what happens next: {said}");
    assert_eq!(
        hold.state(),
        godwinmix_protocol::plugin::wire::InstanceState::Failed,
        "a spent instance reads failed, which is what the supervisor restarts"
    );
}
