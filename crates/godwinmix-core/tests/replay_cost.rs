//! What replay costs on a running programme: the process's CPU time with no
//! replay, with one source angle, with four, and the programme angle read off
//! an encoder an output already runs, plus the memory a ring holds.
//!
//! A measurement, not a check, so it is ignored by default:
//!
//! ```text
//! GMX_REPLAY_CANVAS=1920x1080 cargo test -p godwinmix-core --release --test replay_cost -- --ignored --nocapture
//! ```
//!
//! The sources are `test://smpte`, whose noise patch keeps the encoder busy,
//! at the programme's default 6 Mbit/s. The ring's size is read from
//! `replay.get` and the process's resident memory from `ps`.

use godwinmix_core::config::{Config, OutputConfig, SourceConfig};
use godwinmix_core::mixer::{self, Command, Mixer, MixerHandle};
use godwinmix_core::replay::ReplayCall;
use godwinmix_protocol::replay::ReplayConfig;
use gstreamer as gst;
use std::time::{Duration, Instant};

fn canvas() -> (i32, i32) {
    let text = std::env::var("GMX_REPLAY_CANVAS").unwrap_or_else(|_| "1280x720".into());
    let (w, h) = text.split_once('x').expect("GMX_REPLAY_CANVAS is WIDTHxHEIGHT");
    (w.parse().unwrap(), h.parse().unwrap())
}

fn cpu_seconds() -> f64 {
    godwinmix_govern::load::sys::process_cpu_ns().unwrap_or(0) as f64 / 1e9
}

/// Resident memory in MB, where `ps` can say.
fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output();
    out.ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok()).unwrap_or(0.0) / 1024.0
}

async fn measure(secs: u64) -> f64 {
    let (c0, t0) = (cpu_seconds(), Instant::now());
    tokio::time::sleep(Duration::from_secs(secs)).await;
    (cpu_seconds() - c0) / t0.elapsed().as_secs_f64() * 100.0
}

async fn replay(handle: &MixerHandle, seconds: u32, angles: &[&str]) {
    let cfg = ReplayConfig { seconds, angles: angles.iter().map(|a| a.to_string()).collect(), bitrate_kbps: 0 };
    handle.replay(ReplayCall::Start(cfg)).await.unwrap().expect("replay starts");
}

/// MB a minute one angle's ring holds, from what it holds now.
async fn mb_a_minute(handle: &MixerHandle, angle: &str) -> f64 {
    let (status, _) = handle.replay(ReplayCall::Get).await.unwrap().unwrap();
    let a = status.angles.iter().find(|a| a.id == angle).expect("the angle");
    a.bytes as f64 / a.held_secs.max(0.1) / 1e6 * 60.0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a measurement; see the top of the file"]
async fn an_angle_costs_this_much() {
    gst::init().unwrap();
    let (w, h) = canvas();
    let cfg: Config = toml::from_str(&format!(
        "[canvas]\nwidth = {w}\nheight = {h}\nfps = 30\nsample_rate = 48000\nchannels = 2\n\n[control]\nbind = \"127.0.0.1:0\"\n"
    ))
    .unwrap();
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).expect("build a mixer");
    mix.start().expect("start");
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    let ids = ["cam1", "cam2", "cam3", "cam4"];
    for id in ids {
        let cfg = SourceConfig::bare(id, "test://smpte");
        handle.request(|ack| Command::AddSource(Box::new(cfg), Some(ack))).await.expect("add");
    }
    handle.request(|ack| Command::Take { source: Some("cam1".into()), at_running_time_ms: None, ack: Some(ack) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let (none, rss0) = (measure(10).await, rss_mb());

    replay(&handle, 20, &ids[..1]).await;
    tokio::time::sleep(Duration::from_secs(21)).await;
    let (one, rss1, per_minute) = (measure(10).await, rss_mb(), mb_a_minute(&handle, "cam1").await);
    replay(&handle, 20, &ids).await;
    tokio::time::sleep(Duration::from_secs(21)).await;
    let (four, rss4) = (measure(10).await, rss_mb());
    handle.replay(ReplayCall::Stop).await.unwrap().unwrap();

    let dir = std::env::temp_dir().join(format!("gmx-replay-cost-{}", std::process::id()));
    let mut out = OutputConfig::bare("archive", "record://programme");
    out.params.insert("directory".into(), dir.to_string_lossy().to_string().into());
    handle.request(|ack| Command::AddOutput(Box::new(out), Some(ack))).await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let recording = measure(10).await;
    replay(&handle, 20, &["programme"]).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let shared = measure(10).await;
    let programme_minute = mb_a_minute(&handle, "programme").await;

    handle.send(Command::Shutdown).ok();
    let _ = thread.join();
    let _ = std::fs::remove_dir_all(dir);
    println!("{w}x{h}30, source angles at 6 Mbit/s:");
    println!("  no replay {none:.1} percent of a core, one angle {one:.1}, four angles {four:.1}");
    println!("  one angle {:.1} percent of a core, each of four {:.1}", one - none, (four - none) / 4.0);
    println!("  ring {per_minute:.1} MB a minute; resident memory {rss0:.0} MB, {rss1:.0} MB with one 20 s angle, {rss4:.0} MB with four");
    println!("  programme angle on a recording's encoder: {recording:.1} before, {shared:.1} after, {:.1} for the angle, {programme_minute:.1} MB a minute", shared - recording);
}
