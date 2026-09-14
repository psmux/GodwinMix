//! One end to end run against a live core, for `clients/rust/e2e/run.sh`.
//!
//!     cargo run --example e2e -- http://127.0.0.1:8080 TOKEN
//!
//! Connect, subscribe, receive the snapshot and the flush, add a test source,
//! take it, see `event/program.took`, disconnect. Every step prints ok or the
//! program exits non zero.

use std::process::exit;
use std::time::Duration;

use godwinmix_client::{AddSourceRequest, Client, Event, UI_EVENTS};

macro_rules! step {
    ($name:expr, $body:expr) => {{
        print!("{:<46}", $name);
        match $body {
            Ok(value) => {
                println!("ok");
                value
            }
            Err(e) => {
                println!("FAIL\n    {e}");
                exit(1);
            }
        }
    }};
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base = args.get(1).cloned().unwrap_or_else(|| "http://127.0.0.1:8080".into());
    let token = args.get(2).cloned().filter(|t| !t.is_empty());

    let client = step!("connect to /rpc", Client::connect(&base, token.as_deref()).await);
    let mut events = client.events();

    let result = step!(
        "core.subscribe",
        client.subscribe(&UI_EVENTS, serde_json::json!({"tally": true})).await
    );
    if !result.ignored_ext.is_empty() {
        println!("    the core ignored ext keys: {:?}", result.ignored_ext);
    }

    let seq = step!("event/snapshot then event/flush", await_flush(&client).await);
    println!("    flushed at seq {seq}, {} sources", client.state().status.sources.len());

    let added = step!(
        "source.add test://smpte",
        client
            .source_add(&AddSourceRequest {
                id: Some("e2e-rust".into()),
                uri: "test://smpte".into(),
                name: Some("Rust end to end".into()),
                ..Default::default()
            })
            .await
    );

    step!("program.take", client.take(Some(&added.id)).await);

    let took = step!("event/program.took", wait_for_take(&mut events, &added.id).await);
    println!("    programme is {took}");

    step!("source.remove", client.source_remove(&godwinmix_client::IdRequest { id: added.id.clone() }).await);
    client.close();
    println!("{:<46}ok", "disconnect");
}

async fn await_flush(client: &Client) -> Result<u64, String> {
    tokio::time::timeout(Duration::from_secs(10), client.next_flush())
        .await
        .map_err(|_| "no event/flush in ten seconds".to_string())?
        .map_err(|e| e.to_string())
}

async fn wait_for_take(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
    id: &str,
) -> Result<String, String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return Err(format!("no event/program.took naming {id} in ten seconds"));
        }
        match tokio::time::timeout(left, events.recv()).await {
            Ok(Ok(Event::ProgramTook(took))) => {
                if let Some(source) = took.source {
                    if source == id {
                        return Ok(source);
                    }
                }
            }
            Ok(Ok(_)) => continue,
            Ok(Err(e)) => return Err(e.to_string()),
            Err(_) => return Err(format!("no event/program.took naming {id} in ten seconds")),
        }
    }
}
