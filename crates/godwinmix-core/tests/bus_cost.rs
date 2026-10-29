//! What audio buses cost on a running programme, measured as the whole
//! process's CPU time over a few seconds with no bus and with two.
//!
//! A measurement, not a check, so it is ignored by default:
//!
//! ```text
//! cargo test -p godwinmix-core --release --test bus_cost -- --ignored --nocapture
//! ```
//!
//! Four `test://` sources, each with its tone, on a 320x180 canvas at 30
//! frames, so the picture's share of the CPU is small and steady and the
//! sound's shows. The two buses are an ordinary one with every source sent to
//! it and a mix minus, which between them run both kinds of weight. The
//! programme is measured with no bus, with two, and with none again after they
//! are removed, and the two empty runs are averaged. `GMX_BUS_SOURCES` changes
//! the number of sources and `GMX_BUS_CANVAS` the canvas.

use godwinmix_core::buses::AudioCall;
use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::{self, Command, Mixer, MixerHandle};
use godwinmix_protocol::buses::BusConfig;
use gstreamer as gst;
use std::time::{Duration, Instant};

fn cpu_seconds() -> f64 {
    godwinmix_govern::load::sys::process_cpu_ns().unwrap_or(0) as f64 / 1e9
}

/// CPU used over `secs`, as a percentage of one core.
async fn measure(secs: u64) -> f64 {
    let (c0, t0) = (cpu_seconds(), Instant::now());
    tokio::time::sleep(Duration::from_secs(secs)).await;
    (cpu_seconds() - c0) / t0.elapsed().as_secs_f64() * 100.0
}

async fn add_bus(handle: &MixerHandle, cfg: BusConfig) {
    handle.audio(AudioCall::Add(cfg)).await.expect("the mixer answers").expect("the bus is added");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a measurement; see the top of the file"]
async fn two_buses_cost_this_much() {
    gst::init().unwrap();
    let canvas = std::env::var("GMX_BUS_CANVAS").unwrap_or_else(|_| "320x180".into());
    let (w, h) = canvas.split_once('x').expect("GMX_BUS_CANVAS is WIDTHxHEIGHT");
    let cfg: Config = toml::from_str(&format!(
        "[canvas]\nwidth = {w}\nheight = {h}\nfps = 30\nsample_rate = 48000\nchannels = 2\n\n[control]\nbind = \"127.0.0.1:0\"\n"
    ))
    .unwrap();
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).expect("build a mixer");
    mix.start().expect("start");
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    let n: usize = std::env::var("GMX_BUS_SOURCES").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    let ids: Vec<String> = (1..=n).map(|i| format!("cam{i}")).collect();
    for id in &ids {
        let cfg = SourceConfig::bare(id, "test://smpte");
        handle.request(|ack| Command::AddSource(Box::new(cfg), Some(ack))).await.expect("add");
    }
    handle.request(|ack| Command::Take { source: Some(ids[0].clone()), at_running_time_ms: None, ack: Some(ack) }).await.expect("take");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let none = measure(10).await;

    let mut monitor = BusConfig::named("monitor");
    monitor.sends = ids.iter().map(|id| (id.clone(), 0.8)).collect();
    add_bus(&handle, monitor).await;
    let mut minus = BusConfig::named("guest-return");
    minus.minus = Some(ids[n - 1].clone());
    add_bus(&handle, minus).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let two = measure(10).await;
    for id in ["monitor", "guest-return"] {
        handle.audio(AudioCall::Remove(id.into())).await.expect("the mixer answers").expect("the bus goes");
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    let none = (none + measure(10).await) / 2.0;

    handle.send(Command::Shutdown).ok();
    let _ = thread.join();
    println!(
        "{n} sources at {canvas} 30: no bus {none:.1} percent of a core, two buses {two:.1}, the buses {:.1}",
        two - none
    );
}
