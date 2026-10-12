//! The `file-source` and `live-source` rows: what adding one source to a
//! running core costs.
//!
//! `file-source` used to be a `gst-launch` line, `filesrc ! decodebin !
//! compositor`, which measured GStreamer's defaults rather than the product:
//! as many decoder threads as the machine has cores, a compositor with no
//! latency, none of the mixer's own queues. The budget is about what a person
//! pays when they add a clip, so this builds a core the way every other mixer
//! row does, adds the source with the command `source.add` sends, takes it to
//! programme so it is composited, and measures the difference against the
//! same core a moment before.
//!
//! `live-source` is the same with `test://smpte`, a live 720p30 picture with
//! no decoder: what every source pays the mixer whatever it is, which is the
//! floor under the clip.

use super::{bench_config_with, steady, BenchArgs, Load, Row};
use anyhow::{Context, Result};
use godwinmix_core::config::{MultiviewConfig, SourceConfig};
use godwinmix_core::mixer::{Command, MixerHandle};
use std::path::Path;
use std::time::Duration;

pub async fn file_row(args: &BenchArgs, clip: &Path) -> Result<Row> {
    let uri = godwinmix_core::input::file_uri(clip);
    let params = serde_json::json!({ "at_end": "repeat" });
    let (load, other) = added(args, "bench-clip", &uri, params).await?;
    Ok(Row::new(
        "file-source",
        "Added RSS and CPU per 720p30 file source on programme, no encode",
        "at most 40 MB, at most 15 percent of one core on `pi4`",
        "gmx bench --only file-source".into(),
    )
    .load(load)
    .other(other)
    .note(&file_note(args)))
}

pub async fn live_row(args: &BenchArgs) -> Result<Row> {
    let (load, other) = added(args, "bench-live", "test://smpte", serde_json::json!({})).await?;
    Ok(Row::new(
        "live-source",
        "Added RSS and CPU per live 720p30 source on programme, no decoder, no encode",
        "documented, not budgeted: the floor under every source",
        "gmx bench --only live-source".into(),
    )
    .load(load)
    .other(other)
    .note(
        "A live test picture, so no decoder and no file: what the mixer itself \
         holds for any source. Most of it is the second of frames waiting on the \
         compositor pad, which runs a second behind so a late source never \
         stalls the programme.",
    ))
}

/// A fresh core, measured idle and then with the source on programme.
async fn added(args: &BenchArgs, id: &str, uri: &str, params: serde_json::Value) -> Result<(Load, String)> {
    let mut cfg = bench_config_with(MultiviewConfig { enabled: false, ..Default::default() }, &args.encode);
    if args.decode == "software" {
        cfg.hardware.decode = godwinmix_core::config::Accel::Software;
    }
    let (mut mix, handle, cmd_rx, _bus_rx) = godwinmix_core::mixer::Mixer::build(cfg)?;
    mix.start().context("starting the source row's mixer")?;
    let thread = godwinmix_core::mixer::spawn(mix, cmd_rx, handle.clone());
    let measured = measure(args, (id, uri, params), &handle).await;
    let _ = handle.send(Command::Shutdown);
    let _ = tokio::task::spawn_blocking(move || thread.join()).await;
    measured
}

type Spec<'a> = (&'a str, &'a str, serde_json::Value);

async fn measure(args: &BenchArgs, (id, uri, params): Spec<'_>, handle: &MixerHandle) -> Result<(Load, String)> {
    let before = steady(args.warmup(), args.window()).await;
    let split_before = super::plugins::rss_split();
    let source: SourceConfig = serde_json::from_value(serde_json::json!({
        "id": id,
        "uri": uri,
        "params": params,
    }))?;
    let add = handle.request(|ack| Command::AddSource(Box::new(source), Some(ack)));
    tokio::time::timeout(Duration::from_secs(30), add)
        .await
        .context("adding the source took more than thirty seconds")??;
    let source = Some(id.into());
    let take = handle.request(|ack| Command::Take { source, at_running_time_ms: None, ack: Some(ack) });
    tokio::time::timeout(Duration::from_secs(30), take)
        .await
        .context("taking the source took more than thirty seconds")??;
    let after = steady(args.warmup(), args.window()).await;
    let mut other = format!("{}; {}", super::held::line(id), super::plugins::loaded_line());
    if let Some(((a0, f0), (a1, f1))) = split_before.zip(super::plugins::rss_split()) {
        other = format!("anonymous {:+.1} MB, file backed {:+.1} MB; {other}", a1 - a0, f1 - f0);
    }
    Ok((after.minus(before), other))
}

fn file_note(args: &BenchArgs) -> String {
    let mut note = String::from(
        "The core's own file source, added with `source.add` and taken to \
         programme, against the same idle core just before. In process, not \
         container mode: a sidecar source pays about 41 MB more for its own \
         GStreamer process.",
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
