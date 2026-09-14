//! The mixing engine inside somebody else's program.
//!
//! ```text
//! cargo add godwinmix-core
//! cargo run --example embed
//! ```
//!
//! No HTTP server, no JSON-RPC, no command line: a `Config`, a `Mixer`, a
//! thread to run it on, and the command queue that every client of a full
//! GodwinMix eventually reaches anyway. Add a source, take it, hold the
//! programme for two seconds, shut down.
//!
//! `tests/embed.rs` runs this file, so the example cannot rot.

// `main` is the entry point when this is built as an example and an unused
// function when `tests/embed.rs` includes it as a module.
#![allow(dead_code)]

use anyhow::{Context, Result};
use godwinmix_core::config::SourceConfig;
use godwinmix_core::observe::logs;
use godwinmix_core::prelude::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // The engine's own log layer, which is what a GodwinMix binary installs:
    // `RUST_LOG` sets the starting point and `logs::set_default_level` moves it
    // at runtime. A host program that has its own subscriber skips this.
    logs::init(logs::Options {
        format: logs::Format::Auto,
        node: None,
        env_filter: std::env::var("RUST_LOG").ok(),
    });
    let status = run().await?;
    println!(
        "programme was on {:?} with {} source(s)",
        status.program,
        status.sources.len()
    );
    Ok(())
}

/// Build a mixer, put colour bars on programme, hold it, stop.
///
/// Answers the status read while the programme was live, so a caller (and the
/// test) can assert that something was actually on air.
pub async fn run() -> Result<MixerStatus> {
    // GStreamer once per process, before anything is built. The engine does
    // not do this for you: a host program may have initialised it already.
    gstreamer::init().context("initialising GStreamer")?;

    // Every field of the config has a default, so an empty document is a
    // working 1920x1080 mixer with no sources and no outputs. Reach into the
    // struct for the few things you care about.
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    cfg.canvas.width = 1280;
    cfg.canvas.height = 720;
    cfg.canvas.fps = 30;

    let (mut mix, handle, cmd_rx, mut bus_rx) = Mixer::build(cfg).context("building the mixer")?;
    mix.start().context("starting the mixer")?;

    // Pipeline bus messages go through the same queue as commands, so the
    // engine handles a camera dying and a take on one serialised path.
    {
        let handle = handle.clone();
        tokio::spawn(async move {
            while let Some(ev) = bus_rx.recv().await {
                if handle.send(Command::Bus(ev)).is_err() {
                    return;
                }
            }
        });
    }

    // One thread owns the pipelines for the life of the process. Everything
    // after this point talks to it through `handle`.
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());

    // A source is a `SourceConfig`. `test/source` is the built in colour bars
    // and tone: it needs no network, no file and no browser.
    let source: SourceConfig = toml::from_str(
        r#"
        id = "bars"
        type = "test/source"
        uri = "test://smpte"
        "#,
    )
    .expect("a valid source document");
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle
        .send(Command::AddSource(Box::new(source), Some(tx)))
        .ok();
    rx.await
        .context("the mixer thread stopped")?
        .map_err(anyhow::Error::msg)?;

    // Take it. `None` would cut back to the slate.
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle
        .send(Command::Take {
            source: Some("bars".into()),
            at_running_time_ms: None,
            ack: Some(tx),
        })
        .ok();
    rx.await
        .context("the mixer thread stopped")?
        .map_err(anyhow::Error::msg)?;

    tokio::time::sleep(Duration::from_secs(2)).await;

    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.send(Command::Status(tx)).ok();
    let status = rx.await.context("the mixer thread stopped")?;

    handle.send(Command::Shutdown).ok();
    tokio::task::spawn_blocking(move || thread.join())
        .await
        .ok();
    Ok(status)
}
