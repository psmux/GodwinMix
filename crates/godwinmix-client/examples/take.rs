//! Take a source by id.
//!
//!     cargo run --example take -- http://127.0.0.1:8080 TOKEN cam1
//!
//! The whole of a cut from Rust: connect, subscribe, wait for the first flush
//! so the state is settled, check the source is live, take it.

use std::process::exit;

use godwinmix_client::{Client, Error, UI_EVENTS};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: take <url> <token> <source-id>");
        eprintln!("       take http://127.0.0.1:8080 '' cam1   (a core with no token)");
        exit(2);
    }
    let (base, token, source) = (&args[1], args[2].clone(), &args[3]);
    let token = if token.is_empty() { None } else { Some(token) };

    let client = match Client::connect(base, token.as_deref()).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            exit(1);
        }
    };
    // Only the events this needs. The mosaic, the meters and the tally are all
    // work the core would have to do, and nothing here looks at them.
    if let Err(e) = client.subscribe(&UI_EVENTS, serde_json::json!({})).await {
        eprintln!("{e}");
        exit(1);
    }
    if client.next_flush().await.is_err() {
        eprintln!("the core closed before it sent a snapshot");
        exit(1);
    }

    let state = client.state();
    match state.source(source) {
        None => {
            let known: Vec<&str> = state.status.sources.iter().map(|s| s.id.as_str()).collect();
            eprintln!("no source {source}. This core has: {}", known.join(", "));
            exit(1);
        }
        Some(row) if row.state != "live" => {
            eprintln!("{source} is {}, so taking it would put a hole on air.", row.state);
            exit(1);
        }
        Some(_) => {}
    }

    match client.take(Some(source)).await {
        Ok(program) => println!(
            "{} is on programme at {} ms",
            program.program.as_deref().unwrap_or("the slate"),
            program.running_time_ms
        ),
        Err(e) => {
            eprintln!("{e}");
            if let Some(step) = e.next_step() {
                eprintln!("{step}");
            }
            exit(1);
        }
    }
    client.close();
}

/// Kept so `cargo clippy --all-targets` has something to say about the error
/// path this example leans on.
#[allow(dead_code)]
fn is_worth_retrying(e: &Error) -> bool {
    e.retryable()
}
