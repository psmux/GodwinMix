//! The `file-source` row: what adding one clip to a running core costs.
//!
//! It used to be a `gst-launch` line, `filesrc ! decodebin ! compositor`,
//! which measured GStreamer's defaults rather than the product: as many
//! decoder threads as the machine has cores, a compositor with no latency,
//! none of the mixer's own queues. The budget is about what a person pays
//! when they add a clip, so this builds a core the way every other mixer row
//! does, adds the clip with the command `source.add` sends, takes it to
//! programme so it is composited, and measures the difference against the
//! same core a moment before.

use super::{bench_config_with, steady, BenchArgs, Load, Row};
use anyhow::{Context, Result};
use godwinmix_core::config::{MultiviewConfig, SourceConfig};
use godwinmix_core::mixer::{Command, MixerHandle};
use std::path::Path;
use std::time::Duration;

const ID: &str = "bench-clip";

pub async fn row(args: &BenchArgs, clip: &Path) -> Result<Row> {
    let mut cfg = bench_config_with(MultiviewConfig { enabled: false, ..Default::default() }, &args.encode);
    if args.decode == "software" {
        cfg.hardware.decode = godwinmix_core::config::Accel::Software;
    }
    let (mut mix, handle, cmd_rx, _bus_rx) = godwinmix_core::mixer::Mixer::build(cfg)?;
    mix.start().context("starting the file source mixer")?;
    let thread = godwinmix_core::mixer::spawn(mix, cmd_rx, handle.clone());
    let measured = measure(args, clip, &handle).await;
    let _ = handle.send(Command::Shutdown);
    let _ = tokio::task::spawn_blocking(move || thread.join()).await;
    let (before, after, split) = measured?;
    let mut other = super::plugins::loaded_line();
    if let Some(((a0, f0), (a1, f1))) = split {
        other = format!("anonymous {:+.1} MB, file backed {:+.1} MB; {other}", a1 - a0, f1 - f0);
    }
    Ok(Row::new(
        "file-source",
        "Added RSS and CPU per 720p30 file source on programme, no encode",
        "at most 40 MB, at most 15 percent of one core on `pi4`",
        "gmx bench --only file-source".into(),
    )
    .load(after.minus(before))
    .other(other)
    .note(&note(args)))
}

/// The idle core, then the same core with the clip added and on programme.
type Split = Option<((f64, f64), (f64, f64))>;

async fn measure(args: &BenchArgs, clip: &Path, handle: &MixerHandle) -> Result<(Load, Load, Split)> {
    let before = steady(args.warmup(), args.window()).await;
    let split_before = super::plugins::rss_split();
    let source: SourceConfig = serde_json::from_value(serde_json::json!({
        "id": ID,
        "uri": godwinmix_core::input::file_uri(clip),
        "params": { "at_end": "repeat" },
    }))?;
    let added = handle.request(|ack| Command::AddSource(Box::new(source), Some(ack)));
    tokio::time::timeout(Duration::from_secs(30), added)
        .await
        .context("adding the clip took more than thirty seconds")??;
    let take = handle.request(|ack| Command::Take {
        source: Some(ID.into()),
        at_running_time_ms: None,
        ack: Some(ack),
    });
    tokio::time::timeout(Duration::from_secs(30), take)
        .await
        .context("taking the clip took more than thirty seconds")??;
    let after = steady(args.warmup(), args.window()).await;
    let split = split_before.zip(super::plugins::rss_split());
    Ok((before, after, split))
}

fn note(args: &BenchArgs) -> String {
    let mut note = String::from(
        "The core's own file source, added with `source.add` and taken to \
         programme, against the same idle core just before. In process, not \
         container mode: a sidecar source pays about 41 MB more for its own \
         GStreamer process. The other column is the GStreamer plugins loaded \
         once it was on.",
    );
    let hw_decode = args.decode != "software"
        && godwinmix_core::probe::Backends::probe(
        godwinmix_core::config::Accel::Auto,
        godwinmix_core::config::Accel::Auto,
    )
    .map(|b| b.video_decode.accel != godwinmix_core::config::Accel::Software)
    .unwrap_or(false);
    if hw_decode {
        note.push_str(
            " This machine decodes in hardware, and on macOS VideoToolbox runs in \
             its own XPC process, so the decode does not appear in this process's \
             CPU at all. The number is the mixer's own cost, not the machine's.",
        );
    }
    note
}
