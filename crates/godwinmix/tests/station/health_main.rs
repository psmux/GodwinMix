//! The station's own show, `main`, on a fresh station: no source, so no
//! sound, and no `silence` alarm for it whether its alarms are on or off.
//! A source with sound taken to it and muted is silence, and alarms.

use super::support::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

async fn main_alarms(ws: &mut Ws, id: u64) -> Vec<String> {
    let s = call(ws, id, "show.stats", json!({"ids": ["main"], "fields": ["health"]})).await;
    let alarms = s["result"]["shows"][0]["health"]["alarms"].as_array().cloned().unwrap_or_default();
    alarms.iter().filter_map(|a| a["kind"].as_str().map(str::to_string)).collect()
}

/// Every reading of main's alarms for `secs`, or until `stop` says so.
async fn readings(ws: &mut Ws, secs: u64, stop: impl Fn(&[String]) -> bool) -> Vec<Vec<String>> {
    let (until, mut seen) = (Instant::now() + Duration::from_secs(secs), Vec::new());
    let mut id = 100;
    while Instant::now() < until {
        id += 1;
        seen.push(main_alarms(ws, id).await);
        if stop(seen.last().unwrap()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    seen
}

fn ok(answer: &Value) {
    assert!(answer.get("error").is_none(), "{answer}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn main_on_a_fresh_station_raises_no_silence_and_a_muted_source_on_it_does() {
    let (dir, port) = folder("health-main");
    let st = start(dir, port, &[]).await;
    let mut ws = rpc(&st, "").await;

    // Alarms off, as a fresh station has them, with a one second silence
    // threshold so a wrong alarm would show inside the wait.
    ok(&call(&mut ws, 1, "show.set", json!({"id": "main", "alarms": {"enabled": false, "silence_ms": 1000}})).await);
    let seen = readings(&mut ws, 6, |_| false).await;
    assert!(seen.iter().all(|k| !k.iter().any(|k| k == "silence")), "alarms off: {seen:?}");

    // Alarms on, still nothing on programme: nothing to fall quiet.
    ok(&call(&mut ws, 2, "show.set", json!({"id": "main", "alarms": {"enabled": true, "silence_ms": 1000}})).await);
    let seen = readings(&mut ws, 6, |_| false).await;
    assert!(seen.iter().all(|k| !k.iter().any(|k| k == "silence")), "no source with sound on main: {seen:?}");

    // A tone on programme, muted: that is silence.
    ok(&call(&mut ws, 3, "source.add", json!({"id": "tone", "uri": "test://smpte"})).await);
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let s = call(&mut ws, 6, "core.status", json!({})).await;
        if s["result"]["sources"][0]["state"] == "live" {
            break;
        }
        assert!(Instant::now() < until, "the tone never went live: {s}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    ok(&call(&mut ws, 4, "program.take", json!({"source": "tone"})).await);
    ok(&call(&mut ws, 5, "source.audio.set", json!({"id": "tone", "muted": true})).await);
    let seen = readings(&mut ws, 20, |k| k.iter().any(|k| k == "silence")).await;
    assert!(seen.last().is_some_and(|k| k.iter().any(|k| k == "silence")), "a muted tone on main: {seen:?}");
}
