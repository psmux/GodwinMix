//! Live data feeds against a real mixer and a feed server on loopback.
//!
//! What is checked is what an operator relies on: a value lands in a text
//! and changes when the feed does, an unchanged feed writes nothing (a `304`
//! costs no body and no write), a server that hangs costs only its own feed,
//! a path that picks nothing says what was there, and pushed feeds get
//! through. No mocks on the mixer side, and nothing reaches the internet.

mod core;
mod server;

use crate::core::Core;
use godwinmix_protocol::types::Event;
use serde_json::json;
use server::Server;
use std::time::{Duration, Instant};

const SCORE: &str = r#"{"match":{"home":"Leeds","away":"Hull","home_score":2,"away_score":0},"updated":"19:00"}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_binding_writes_when_the_value_changes_and_not_otherwise() {
    let core = Core::start("change").await;
    let server = Server::start().await;
    server.put("application/json", SCORE);
    core.text("score", "0 : 0").await;
    core.ok("feed.add", json!({ "id": "scores", "address": server.url("/feed"), "interval_s": 3600 })).await;
    core.until("the feed is read", || async { core.feed("scores").await["fetches"] == 1 }).await;
    let bound = core
        .ok("feed.binding.add", json!({
            "feed": "scores", "select": "match", "template": "{home} {home_score} : {away_score} {away}",
            "to": { "source": "score", "path": "params.text" }
        }))
        .await;
    assert_eq!(bound["id"], "score-text", "an id is made from the target: {bound}");
    assert_eq!(bound["writes"], 1);
    assert_eq!(core.param("score", "text").await, "Leeds 2 : 0 Hull");

    // The same version again: the server says 304 and nothing is written.
    core.ok("feed.refresh", json!({ "id": "scores" })).await;
    core.until("a second fetch", || async { core.feed("scores").await["fetches"] == 2 }).await;
    assert_eq!(server.served.lock().not_modified, 1, "the ETag went back and the server answered 304");
    assert_eq!(core.feed("scores").await["not_modified"], 1);
    assert_eq!(core.binding("score-text").await["writes"], 1, "a 304 writes nothing");

    // A new version whose bound fields are the same: read, and still no write.
    server.put("application/json", &SCORE.replace("19:00", "19:01"));
    core.ok("feed.refresh", json!({ "id": "scores" })).await;
    core.until("a third fetch", || async { core.feed("scores").await["fetches"] == 3 }).await;
    assert_eq!(core.binding("score-text").await["writes"], 1, "an unrelated field changed, so nothing is written");

    // A goal.
    server.put("application/json", &SCORE.replace("\"home_score\":2", "\"home_score\":3"));
    core.ok("feed.refresh", json!({ "id": "scores" })).await;
    core.until("the goal is written", || async { core.binding("score-text").await["writes"] == 2 }).await;
    assert_eq!(core.param("score", "text").await, "Leeds 3 : 0 Hull");
    assert_eq!(core.ok("source.get", json!({ "id": "score" })).await["state"], "live", "written in place, never rebuilt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rss_headlines_fill_a_ticker_and_a_sheet_fills_a_strap() {
    let core = Core::start("rss").await;
    let server = Server::start().await;
    let items: String = (1..=12).map(|n| format!("<item><title>Headline {n}</title></item>")).collect();
    server.put("application/rss+xml", &format!("<rss><channel><title>News</title>{items}</channel></rss>"));
    core.ok("source.add", json!({ "id": "crawl", "uri": "ticker:Waiting" })).await;
    core.ok("feed.add", json!({ "id": "news", "address": server.url("/feed"), "interval_s": 3600 })).await;
    core.until("the feed is read", || async { core.feed("news").await["state"] == "ok" }).await;
    core.ok("feed.binding.add", json!({ "feed": "news", "select": "items[].title", "limit": 10, "to": { "source": "crawl", "path": "params.items" } })).await;
    let want: Vec<String> = (1..=10).map(|n| format!("Headline {n}")).collect();
    assert_eq!(core.param("crawl", "items").await, json!(want));

    server.put("text/csv", "Name,Title\nAda Lovelace,Analyst\nGrace Hopper,Rear Admiral\n");
    core.text("strap", "Name").await;
    core.ok("feed.add", json!({ "id": "guests", "address": server.url("/feed"), "interval_s": 3600 })).await;
    core.until("the sheet is read", || async { core.feed("guests").await["state"] == "ok" }).await;
    core.ok("feed.binding.add", json!({ "feed": "guests", "select": "rows[1]", "template": "{Name}\n{Title}", "to": { "source": "strap", "path": "params.text" } })).await;
    assert_eq!(core.param("strap", "text").await, "Grace Hopper\nRear Admiral");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scene_parameter_follows_a_feed_and_a_lost_source_is_retried() {
    let core = Core::start("param").await;
    let server = Server::start().await;
    server.put("application/json", SCORE);
    core.ok("feed.add", json!({ "id": "scores", "address": server.url("/feed"), "interval_s": 3600 })).await;
    core.until("the feed is read", || async { core.feed("scores").await["state"] == "ok" }).await;
    core.ok("feed.binding.add", json!({ "feed": "scores", "select": "match.home", "to": { "scene_param": "home" } })).await;
    let params = core.ok("scene.params.get", json!({})).await;
    assert!(params.to_string().contains("Leeds"), "{params}");

    // A source that goes away: the write fails once, is said once, and is
    // tried again on the next fetch even though the feed has not changed.
    core.text("strap", "-").await;
    let mut events = core.app.mixer.subscribe();
    core.ok("feed.binding.add", json!({ "id": "away", "feed": "scores", "select": "match.away", "to": { "source": "strap", "path": "params.text" } })).await;
    core.ok("source.remove", json!({ "id": "strap" })).await;
    server.put("application/json", &SCORE.replace("Hull", "York"));
    core.ok("feed.refresh", json!({ "id": "scores" })).await;
    core.until("the write fails", || async { core.binding("away").await["last_error"].is_string() }).await;
    let failed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(env) = events.recv().await {
                if let Event::FeedFailed { binding: Some(b), .. } = env.event {
                    return b;
                }
            }
        }
    })
    .await
    .expect("feed.failed was sent for the binding");
    assert_eq!(failed, "away");
    core.text("strap", "-").await;
    core.ok("feed.refresh", json!({ "id": "scores" })).await;
    core.until("the value lands on the new source", || async { core.param("strap", "text").await == "York" }).await;
    assert!(core.binding("away").await["last_error"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_hangs_times_out_without_holding_up_another_feed() {
    let core = Core::start("hang").await;
    let server = Server::start().await;
    server.put("application/json", SCORE);
    let mut events = core.app.mixer.subscribe();
    let started = Instant::now();
    core.ok("feed.add", json!({ "id": "slow", "address": server.url("/hang"), "timeout_s": 1, "interval_s": 3600 })).await;
    core.ok("feed.add", json!({ "id": "quick", "address": server.url("/feed"), "interval_s": 3600 })).await;
    core.until("the quick feed is read", || async { core.feed("quick").await["state"] == "ok" }).await;
    let quick = started.elapsed();
    assert!(quick < Duration::from_millis(900), "the quick feed waited on the slow one: {quick:?}");
    assert_eq!(core.feed("slow").await["state"], "starting", "the slow feed is still waiting on its server");
    core.until("the slow feed times out", || async { core.feed("slow").await["state"] == "failing" }).await;
    let slow = core.feed("slow").await;
    assert!(slow["last_error"].as_str().unwrap().contains("no answer within 1 s"), "{slow}");
    let failed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(env) = events.recv().await {
                if let Event::FeedFailed { id, binding: None, .. } = env.event {
                    return id;
                }
            }
        }
    })
    .await
    .expect("feed.failed was sent");
    assert_eq!(failed, "slow");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_path_that_selects_nothing_shows_what_is_there() {
    let core = Core::start("path").await;
    let server = Server::start().await;
    server.put("application/json", SCORE);
    let tried = core.ok("feed.test", json!({ "address": server.url("/feed") })).await;
    assert_eq!(tried["format"], "json");
    assert_eq!(tried["keys"], json!(["match", "updated"]));
    assert!(tried["paths"].as_array().unwrap().iter().any(|p| p["path"] == "match.home_score" && p["example"] == 2), "{tried}");

    let picked = core.ok("feed.test", json!({ "address": server.url("/feed"), "select": "match.home", "template": "Home: {}" })).await;
    assert_eq!(picked["value"], "Home: Leeds");

    let err = core.call("feed.test", json!({ "address": server.url("/feed"), "select": "matches[0].home" })).await.unwrap_err();
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("`matches[0].home` selects nothing") && err.message.contains("It has: match, updated"), "{}", err.message);
    assert_eq!(err.data["top_level_keys"], json!(["match", "updated"]));
    let err = core.call("feed.test", json!({ "address": server.url("/feed"), "select": "match.hom" })).await.unwrap_err();
    assert_eq!(err.data["stopped_at"], "match");
    assert_eq!(err.data["keys_there"], json!(["away", "away_score", "home", "home_score"]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_websocket_and_an_event_stream_push_values_through() {
    let core = Core::start("push").await;
    let server = Server::start().await;
    core.text("live", "-").await;
    core.text("sse", "-").await;
    core.ok("feed.add", json!({ "id": "socket", "address": format!("ws://{}/ws", server.base) })).await;
    core.ok("feed.add", json!({ "id": "stream", "address": server.url("/events"), "format": "sse" })).await;
    core.until("both connect", || async { server.receivers() == 2 }).await;
    server.push(r#"{"score": 1}"#);
    core.until("the first message is read", || async { core.feed("socket").await["state"] == "ok" && core.feed("stream").await["state"] == "ok" }).await;
    core.ok("feed.binding.add", json!({ "feed": "socket", "select": "score", "template": "Score {}", "to": { "source": "live", "path": "params.text" } })).await;
    core.ok("feed.binding.add", json!({ "feed": "stream", "select": "score", "to": { "source": "sse", "path": "params.text" } })).await;
    assert_eq!(core.param("live", "text").await, "Score 1");
    server.push(r#"{"score": 2}"#);
    core.until("the second message is written", || async { core.param("live", "text").await == "Score 2" }).await;
    core.until("the event stream writes it too", || async { core.param("sse", "text").await == "2" }).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feeds_are_kept_beside_the_config_with_their_keys_sealed() {
    let core = Core::start("keep").await;
    let server = Server::start().await;
    server.put("application/json", SCORE);
    core.text("score", "-").await;
    core.ok("feed.add", json!({ "id": "scores", "address": server.url("/feed"), "interval_s": 60, "headers": { "x-api-key": "k-12345" } })).await;
    core.until("the feed is read", || async { core.feed("scores").await["state"] == "ok" }).await;
    assert_eq!(server.served.lock().key.as_deref(), Some("k-12345"), "the header went with the request");
    assert_eq!(core.feed("scores").await["headers"]["x-api-key"], "__secret__", "a key never comes back");
    core.ok("feed.binding.add", json!({ "id": "home", "feed": "scores", "select": "match.home", "to": { "source": "score", "path": "params.text" } })).await;

    let file = core.path.with_file_name("godwinmix.feeds.json");
    let text = std::fs::read_to_string(&file).expect("the feeds file is written");
    assert!(!text.contains("k-12345") && text.contains("__secret__"), "{text}");

    // What a restart reads: the same feeds and bindings, the key unsealed.
    let reopened = godwinmix::feeds::Feeds::open(&core.path);
    let list = reopened.list();
    assert_eq!(list.feeds[0].spec.id, "scores");
    assert_eq!(list.bindings[0].spec.id, "home");
    reopened.start(godwinmix::feeds::Ctx { app: core.app.clone(), snapshots: core.snapshots.clone() });
    let before = server.served.lock().requests;
    core.until("the reopened feed fetches", || async { server.served.lock().requests > before }).await;
    assert_eq!(server.served.lock().key.as_deref(), Some("k-12345"), "the sealed key was read back");

    // Keeping it on a set, and refusing what is not a network address.
    core.ok("feed.set", json!({ "id": "scores", "headers": { "x-api-key": "__secret__" } })).await;
    let err = core.call("feed.add", json!({ "id": "local", "address": "file:///etc/passwd" })).await.unwrap_err();
    assert!(err.message.contains("network only") && err.data["field"] == "address", "{err:?}");
    let err = core.call("feed.add", json!({ "id": "fast", "address": server.url("/feed"), "interval_s": 1 })).await.unwrap_err();
    assert!(err.message.contains("at least 5 s"), "{err:?}");
    let err = core.call("feed.binding.add", json!({ "feed": "scores", "select": "match.home", "to": { "source": "nope", "path": "params.text" } })).await.unwrap_err();
    assert_eq!(err.code, -32004, "{err:?}");
    let removed = core.ok("feed.remove", json!({ "id": "scores" })).await;
    assert_eq!(removed["bindings"], json!(["home"]));
}
