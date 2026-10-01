//! The station against the real binary: a single config run in place as
//! `main`, a call and an event through the relay and what the relay adds to
//! each, and a show killed while another runs, with the governor's book
//! across the two processes. See `docs/reference/shows.md`.

mod direct;
#[cfg(unix)]
mod isolation;
mod project;
mod support;
mod switch;

use serde_json::json;
use std::time::{Duration, Instant};
use support::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_single_config_runs_in_place_as_main_and_a_call_and_an_event_pass_the_relay() {
    let (dir, port) = folder("single");
    let st = start(dir.clone(), port, &[]).await;

    let list = get(&st, "/api/v1/shows").await;
    assert_eq!(list["current"], "main", "{list}");
    assert_eq!(list["shows"].as_array().map(Vec::len), Some(1), "{list}");
    assert_eq!(list["shows"][0]["state"], "running", "{list}");
    assert!(!dir.join("shows.json").exists(), "one show writes no list");
    assert!(!dir.join("shows").exists(), "main is run in place, not copied");

    let mut ws = rpc(&st, "").await;
    let status = call(&mut ws, 1, "core.status", json!({})).await;
    assert!(status["result"]["uptime_secs"].is_u64(), "a show's method through the relay: {status}");
    call(&mut ws, 2, "core.subscribe", json!({"events": ["scene.*", "show.*"]})).await;
    let added = call(&mut ws, 3, "scene.add", json!({"name": "Relay test"})).await;
    assert!(added.get("result").is_some(), "{added}");
    let patch = event(&mut ws, "scene.patch", Duration::from_secs(10), |_| true).await;
    assert!(patch.is_some(), "a show's event came back through the relay");
    let shows = call(&mut ws, 4, "show.list", json!({})).await;
    assert_eq!(shows["result"]["current"], "main", "a station method on the same socket: {shows}");

    // The scene the call made is in main's own store, beside the config.
    let beside = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).any(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        name.starts_with("godwinmix.") && name != "godwinmix.toml" && std::fs::read_to_string(e.path()).is_ok_and(|t| t.contains("Relay test"))
    });
    assert!(beside, "main keeps its data where a single process core kept it");
}

/// What the relay adds: the same calls and events on a station and on a
/// single process core, side by side. Printed for the report; the assertion
/// is only that it stays small.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_relay_adds_little_to_a_call_or_an_event() {
    let (dir, port) = folder("relayed");
    let station = start(dir, port, &[]).await;
    let (dir, port) = folder("direct");
    let direct = start(dir, port, &["--show", "solo"]).await;
    let mut out = Vec::new();
    for (label, core) in [("direct", &direct), ("station", &station)] {
        let mut ws = rpc(core, "").await;
        call(&mut ws, 1, "core.subscribe", json!({"events": ["scene.*"]})).await;
        let mut calls = Vec::new();
        for i in 0..60 {
            let t = Instant::now();
            call(&mut ws, 100 + i, "core.status", json!({})).await;
            calls.push(t.elapsed().as_micros());
        }
        let mut events = Vec::new();
        for i in 0..20 {
            let t = Instant::now();
            call(&mut ws, 1000 + i, "scene.add", json!({"name": format!("s{i}")})).await;
            event(&mut ws, "scene.patch", Duration::from_secs(10), |_| true).await.expect("a patch");
            events.push(t.elapsed().as_micros());
        }
        out.push((label, median(calls), median(events)));
    }
    for (label, c, e) in &out {
        eprintln!("relay latency {label}: core.status median {c} us, scene.add to event/scene.patch median {e} us");
    }
    let (call_added, event_added) = (out[1].1 as i128 - out[0].1 as i128, out[1].2 as i128 - out[0].2 as i128);
    eprintln!("relay latency added: call {call_added} us, event {event_added} us");
    assert!(call_added < 20_000 && event_added < 20_000, "the relay should add well under 20 ms: {out:?}");
}
