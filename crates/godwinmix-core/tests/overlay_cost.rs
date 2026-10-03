//! What a text, a ticker, a transparent picture and a transparent clip cost on
//! a running 720p30 programme, against the same programme without them.
//!
//! Run by hand, in release, on the machine being asked about:
//!
//! ```sh
//! GMX_ALPHA_CLIP=/path/to/clip.webm cargo test --release -p godwinmix-core \
//!     --test overlay_cost -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Prints the process CPU over ten seconds as a percentage of one core, and the
//! resident memory, for each case. The encoder is off (nothing reads it), so
//! what is measured is the sources and the compositing.

// Process CPU is read with getrusage, which Windows does not have.
#![cfg(unix)]

use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::slots::{Placement, Sizing};
use godwinmix_core::mixer::{self, Command, Mixer, MixerHandle, ProgramScene};
use gstreamer as gst;
use std::time::{Duration, Instant};

const W: i32 = 1280;
const H: i32 = 720;

fn cpu_seconds() -> f64 {
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    let t = |v: libc::timeval| v.tv_sec as f64 + v.tv_usec as f64 / 1e6;
    t(u.ru_utime) + t(u.ru_stime)
}

fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output();
    out.ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok()).unwrap_or(0.0) / 1024.0
}

fn place(id: &str, x: i32, y: i32, w: i32, h: i32, sizing: Sizing) -> Placement {
    let canvas = godwinmix_core::caps::CanvasCaps { width: W, height: H, fps: gst::Fraction::new(30, 1), sample_rate: 48000, channels: 2 };
    Placement { xpos: x, ypos: y, width: w, height: h, sizing, ..Placement::full_canvas(id.into(), &canvas) }
}

async fn run_case(name: &str, extra: Option<(SourceConfig, Placement)>) {
    gst::init().unwrap();
    let cfg: Config = toml::from_str(&format!(
        "[canvas]\nwidth = {W}\nheight = {H}\nfps = 30\nsample_rate = 48000\nchannels = 2\n\n[control]\nbind = \"127.0.0.1:0\"\n\n[multiview]\nenabled = false\n"
    ))
    .unwrap();
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    let add = |h: &MixerHandle, c: SourceConfig| {
        let h = h.clone();
        async move { h.request(|ack| Command::AddSource(Box::new(c), Some(ack))).await.unwrap() }
    };
    add(&handle, SourceConfig::bare("bg", "test://smpte")).await;
    let mut placements = vec![place("bg", 0, 0, W, H, Sizing::Fill)];
    if let Some((cfg, p)) = extra {
        add(&handle, cfg).await;
        placements.push(p);
    }
    let scene = ProgramScene { name: "cost".into(), placements };
    handle
        .request(|ack| Command::TakeScene { scene: Box::new(scene), at_running_time_ms: None, duration_ms: None, transition: None, ack: Some(ack) })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let (c0, t0) = (cpu_seconds(), Instant::now());
    tokio::time::sleep(Duration::from_secs(10)).await;
    let cpu = (cpu_seconds() - c0) / t0.elapsed().as_secs_f64() * 100.0;
    let rss = rss_mb();
    handle.send(Command::Shutdown).ok();
    let _ = thread.join();
    println!("{name:<44} {cpu:>6.1} % of a core   {rss:>6.0} MB resident");
}

fn source(id: &str, uri: &str, params: &str) -> SourceConfig {
    let mut c = SourceConfig::bare(id, uri);
    c.params = toml::from_str(params).unwrap();
    c
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn what_each_transparent_source_costs() {
    let dir = std::env::temp_dir().join(format!("gmx-cost-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let logo = dir.join("logo.png");
    let full = dir.join("full.png");
    write_png(&logo, 400, 200);
    write_png(&full, W as u32, H as u32);
    run_case("programme alone (bars, 720p30)", None).await;
    let strap = source("strap", "text:Ada Lovelace\\nAnalyst", "size = 44\nshadow = true");
    run_case("+ static text, lower third box", Some((strap, place("strap", 64, 500, 760, 140, Sizing::Contain)))).await;
    let crawl = source("crawl", "ticker:Markets up two percent   Rain later in the north", "size = 30\nspeed = 120");
    run_case("+ ticker, full width bar 1280x48", Some((crawl, place("crawl", 0, 672, W, 48, Sizing::Fill)))).await;
    let png = source("logo", &logo.to_string_lossy(), "");
    run_case("+ transparent PNG 400x200 at its size", Some((png, place("logo", 840, 40, 400, 200, Sizing::Fill)))).await;
    let big = source("big", &full.to_string_lossy(), "");
    run_case("+ transparent PNG 1280x720, full frame", Some((big, place("big", 0, 0, W, H, Sizing::Fill)))).await;
    let opaque = source("flat", &logo.to_string_lossy(), "alpha = false");
    run_case("  (the same 400x200 PNG drawn flat, for scale)", Some((opaque, place("flat", 840, 40, 400, 200, Sizing::Fill)))).await;
    if let Ok(clip) = std::env::var("GMX_ALPHA_CLIP") {
        let c = source("clip", &clip, "");
        run_case("+ transparent clip, full frame", Some((c, place("clip", 0, 0, W, H, Sizing::Fill)))).await;
        let c = source("clip", &clip, "");
        run_case("+ transparent clip at 640x360", Some((c, place("clip", 0, 0, 640, 360, Sizing::Fill)))).await;
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A picture whose left half is an opaque gradient and right half clear.
fn write_png(path: &std::path::Path, w: u32, h: u32) {
    gst::init().unwrap();
    let desc = format!(
        "videotestsrc num-buffers=1 pattern=smpte ! video/x-raw,format=AYUV,width={},height={h} ! videobox right=-{} border-alpha=0 ! videoconvert ! pngenc ! filesink location=\"{}\"",
        w / 2,
        w - w / 2,
        path.display()
    );
    let p = gst::parse::launch(&desc).unwrap();
    use gstreamer::prelude::*;
    p.set_state(gst::State::Playing).unwrap();
    p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Eos]);
    p.set_state(gst::State::Null).unwrap();
}
