//! Two hundred shows: made in one call, and read once a second without the
//! station asking any show anything. Printed for the report; the assertion
//! is only that a read stays well under what a page polling once a second
//! can live with.

use super::support::*;
use serde_json::{json, Value};
use std::time::Instant;

fn median(mut v: Vec<u128>) -> u128 {
    v.sort_unstable();
    v[v.len() / 2]
}

async fn time(ws: &mut Ws, id: u64, method: &str) -> u128 {
    let t = Instant::now();
    let answer = call(ws, id, method, json!({})).await;
    assert_eq!(answer["result"]["shows"].as_array().map(Vec::len), Some(201), "{method}");
    t.elapsed().as_micros()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_hundred_shows_are_made_in_one_call_and_read_cheaply() {
    let (dir, port) = folder("many");
    let st = start(dir, port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let shows: Vec<Value> = (0..200)
        .map(|i| json!({"name": format!("Feed {i}"), "compositing": false, "input": {"uri": format!("udp://@239.2.{}.{}:5000", i / 250, i % 250)},
                        "outputs": [{"uri": format!("srt://10.0.0.9:{}", 20000 + i)}]}))
        .collect();
    let t = Instant::now();
    let made = call(&mut ws, 1, "show.add_many", json!({"shows": shows, "dry_run": false})).await;
    eprintln!("show.add_many of 200 direct shows: {} ms", t.elapsed().as_millis());
    assert_eq!(made["result"]["added"].as_array().map(Vec::len), Some(200), "{made}");

    let mut list = Vec::new();
    let mut stats = Vec::new();
    for i in 0..20 {
        list.push(time(&mut ws, 100 + i, "show.list").await);
        stats.push(time(&mut ws, 200 + i, "show.stats").await);
    }
    let (l, s) = (median(list), median(stats));
    eprintln!("201 shows: show.list median {l} us, show.stats median {s} us, through the socket");
    assert!(l < 250_000 && s < 250_000, "a read of 201 shows should take well under a quarter second: list {l} us, stats {s} us");
}
