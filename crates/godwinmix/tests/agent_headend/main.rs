//! The headend case, run the way an agent runs it: "I have channels coming
//! from a headend; add them and run them." A real station, `gmx mcp` over
//! stdio, and a harness that only speaks MCP to it.
//!
//! It measures what the cheap path costs (tool calls, bytes each way, time)
//! and prints it, because that is what `skills/godwinmix-operate` tells an
//! agent to expect. Every wait has a limit.

mod agent;
mod cli;
// Shared with the station tests: start a real station, wait for it, stop it.
#[allow(dead_code)]
#[path = "../station/support.rs"]
mod support;

use agent::Agent;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// Twenty feeds as a headend's channel list would give them: one UDP input
/// each, one copy output each. Loopback unicast, so the test needs no
/// multicast route and nothing has to be sending for the shows to be made.
fn headend(n: u16) -> Vec<Value> {
    (0..n)
        .map(|i| {
            json!({
                "name": format!("Channel {}", i + 1),
                "compositing": false,
                "input": { "uri": format!("udp://@127.0.0.1:{}", 25_000 + i) },
                "outputs": [{ "uri": format!("udp://127.0.0.1:{}", 26_000 + i) }]
            })
        })
        .collect()
}

fn names(tools: &Value) -> Vec<String> {
    tools.as_array().into_iter().flatten().filter_map(|t| t["name"].as_str().map(String::from)).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_adds_twenty_feeds_in_two_calls_and_reads_them_in_one() {
    let (dir, port) = support::folder("agent-headend");
    let st = support::start(dir, port, &[]).await;
    let mut agent = Agent::connect(&st.url);

    let init = agent.request("initialize", json!({ "protocolVersion": "2025-06-18" })).await;
    assert!(init["result"]["instructions"].as_str().unwrap_or_default().contains("add_shows"));
    let listed = agent.request("tools/list", json!({})).await;
    let hot = names(&listed["result"]["tools"]);
    let list_bytes = listed["result"].to_string().len();
    assert!(hot.contains(&"add_shows".to_string()), "add_shows is hot: {hot:?}");

    // An agent that does not trust the hot list looks; this is what it pays.
    let before_search = agent.read_bytes;
    let found = agent.tool("search_tools", json!({ "query": "add many shows from a list of feeds" })).await;
    assert!(found.to_string().contains("add_shows"), "{found}");
    let search_bytes = agent.read_bytes - before_search;
    let before_work = agent.read_bytes;

    let t = Instant::now();
    let shows = headend(20);
    let plan = agent.tool("add_shows", json!({ "shows": shows, "dry_run": true })).await;
    assert_eq!(plan["added"].as_array().map(Vec::len), Some(20), "the dry run would add all 20: {plan}");
    assert!(plan["plan"]["fits"].as_bool().unwrap_or(false), "copies fit: {plan}");
    let before = agent.tool("list_shows", json!({})).await;
    assert_eq!(before["shows"].as_array().map(Vec::len), Some(1), "a dry run adds nothing: {before}");

    let applied = agent.tool("add_shows", json!({ "shows": shows, "dry_run": false })).await;
    let added = applied["added"].as_array().cloned().unwrap_or_default();
    assert_eq!(added.len(), 20, "{applied}");
    let add_ms = t.elapsed().as_millis();

    // Read them back until every one has a health, at most 30 s.
    let ids: Vec<Value> = added.clone();
    let t = Instant::now();
    let mut reads = 0;
    let stats = loop {
        reads += 1;
        let s = agent.tool("show_stats", json!({ "ids": ids })).await;
        let healthy = s["shows"].as_array().map(|a| a.iter().filter(|x| x["health"]["state"].is_string()).count());
        if healthy == Some(20) {
            break s;
        }
        assert!(t.elapsed() < Duration::from_secs(30), "show_stats never named all 20: {s}");
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    let one_read = stats.to_string().len();
    let work_bytes = agent.read_bytes - before_work;

    eprintln!(
        "agent headend run: tools/list {list_bytes} B for {} hot tools; 20 shows added in {add_ms} ms \
         (dry run, list, apply); {reads} show_stats read(s) until all 20 had a health, one read of 20 \
         is {one_read} B; the search cost {search_bytes} B and the work itself (dry run, list, apply, \
         stats) {work_bytes} B read; total {} tool calls, {} B sent, {} B read, {} ms wall",
        hot.len(),
        agent.tool_calls,
        agent.sent_bytes,
        agent.read_bytes,
        agent.started.elapsed().as_millis()
    );
    assert!(agent.tool_calls <= 8, "the cheap path is a handful of calls, not one per show");
}
