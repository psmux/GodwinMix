//! What a graphic template costs on a running 720p30 programme: a lower
//! third and a breaking news bar, held still and with a field changed once a
//! second, against the same programme without them.
//!
//! Run by hand, in release, on the machine being asked about:
//!
//! ```sh
//! cargo test --release -p godwinmix-core --test template_cost -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Prints the process CPU over ten seconds as a percentage of one core. The
//! encoder is off (nothing reads it), so what is measured is the sources and
//! the compositing, the same as `overlay_cost`.

// Process CPU is read with getrusage, which Windows does not have.
#![cfg(unix)]

use godwinmix_core::config::{Config, Params, SourceConfig};
use godwinmix_core::mixer::slots::{Placement, Sizing};
use godwinmix_core::mixer::{self, Command, Mixer, ProgramScene};
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

fn full(id: &str) -> Placement {
    let canvas = godwinmix_core::caps::CanvasCaps { width: W, height: H, fps: gst::Fraction::new(30, 1), sample_rate: 48000, channels: 2 };
    Placement { sizing: Sizing::Fill, ..Placement::full_canvas(id.into(), &canvas) }
}

fn fields(field: &str, value: &str) -> Params {
    toml::from_str(&format!("[fields]\n{field} = {value:?}")).unwrap()
}

/// One case: the bars, and the template if there is one. With `changing`,
/// `field` is given a new value once a second while the CPU is counted.
async fn run_case(name: &str, template: Option<(&str, &str)>, changing: bool) {
    gst::init().unwrap();
    let cfg: Config = toml::from_str(&format!(
        "[canvas]\nwidth = {W}\nheight = {H}\nfps = 30\nsample_rate = 48000\nchannels = 2\n\n[control]\nbind = \"127.0.0.1:0\"\n\n[multiview]\nenabled = false\n"
    ))
    .unwrap();
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    let add = |c: SourceConfig| {
        let h = handle.clone();
        async move { h.request(|ack| Command::AddSource(Box::new(c), Some(ack))).await.unwrap() }
    };
    add(SourceConfig::bare("bg", "test://smpte")).await;
    let mut placements = vec![full("bg")];
    if let Some((t, _)) = template {
        add(SourceConfig::bare("gfx", &format!("template:{t}"))).await;
        placements.push(full("gfx"));
    }
    let scene = ProgramScene { name: "cost".into(), placements };
    handle
        .request(|ack| Command::TakeScene { scene: Box::new(scene), at_running_time_ms: None, duration_ms: None, transition: None, ack: Some(ack) })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let (c0, t0) = (cpu_seconds(), Instant::now());
    for second in 0..10 {
        if let (true, Some((_, field))) = (changing, template) {
            let value = format!("Headline number {second} as the count goes on into the night");
            let _ = handle.configure_source("gfx".into(), fields(field, &value)).await;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let cpu = (cpu_seconds() - c0) / t0.elapsed().as_secs_f64() * 100.0;
    handle.send(Command::Shutdown).ok();
    let _ = thread.join();
    println!("{name:<52} {cpu:>6.1} % of a core");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn what_a_template_costs() {
    run_case("programme alone (bars, 720p30)", None, false).await;
    run_case("+ news lower third, held", Some(("news-lower-third", "name")), false).await;
    run_case("+ news lower third, a field changed every second", Some(("news-lower-third", "name")), true).await;
    run_case("+ breaking news bar, held", Some(("breaking-news", "headline")), false).await;
    run_case("+ breaking news bar, a field changed every second", Some(("breaking-news", "headline")), true).await;
}
