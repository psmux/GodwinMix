//! What a keyed camera costs on a running programme, measured as the whole
//! process's CPU time over a few seconds with the key on and with it off.
//!
//! A measurement, not a check, so it is ignored by default:
//!
//! ```text
//! GMX_KEY_CANVAS=1280x720 cargo test -p godwinmix-core --release --test key_cost -- --ignored --nocapture
//! ```
//!
//! The presenter is `test://smpte` keyed on its green bar, so six sevenths of
//! it stay solid: close to the most a key can cost, since a solid pixel is
//! the one that has to be drawn. `GMX_KEY_SOURCE=studio` keys a still of a
//! presenter-shaped figure on a green screen instead, about a third solid,
//! which is closer to a real shot.

// getrusage is a Unix call; the key costs the same on Windows.
#![cfg(unix)]

use godwinmix_core::config::{Config, Params, SourceConfig};
use godwinmix_core::mixer::slots::{ItemFilter, Placement, Sizing};
use godwinmix_core::mixer::{self, Command, Mixer, MixerHandle, ProgramScene};
use gstreamer as gst;
use std::time::{Duration, Instant};

fn canvas() -> (i32, i32) {
    let text = std::env::var("GMX_KEY_CANVAS").unwrap_or_else(|_| "1280x720".into());
    let (w, h) = text.split_once('x').expect("GMX_KEY_CANVAS is WIDTHxHEIGHT");
    (w.parse().unwrap(), h.parse().unwrap())
}

fn cpu_seconds() -> f64 {
    // SAFETY: getrusage writes into the struct it is given and nothing else.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let t = |tv: libc::timeval| tv.tv_sec as f64 + tv.tv_usec as f64 / 1e6;
    t(usage.ru_utime) + t(usage.ru_stime)
}

fn placement(source: &str, w: i32, h: i32, filters: Vec<ItemFilter>) -> Placement {
    let canvas = godwinmix_core::caps::CanvasCaps { width: w, height: h, fps: gst::Fraction::new(30, 1), sample_rate: 48000, channels: 2 };
    Placement { sizing: Sizing::Fill, filters, ..Placement::full_canvas(source.into(), &canvas) }
}

async fn take(handle: &MixerHandle, placements: Vec<Placement>) {
    let scene = ProgramScene { name: "cost".into(), placements };
    handle
        .request(|ack| Command::TakeScene { scene: Box::new(scene), at_running_time_ms: None, duration_ms: None, transition: None, ack: Some(ack) })
        .await
        .expect("take the scene");
}

/// CPU used over `secs`, as a percentage of one core.
async fn measure(secs: u64) -> f64 {
    let (c0, t0) = (cpu_seconds(), Instant::now());
    tokio::time::sleep(Duration::from_secs(secs)).await;
    (cpu_seconds() - c0) / t0.elapsed().as_secs_f64() * 100.0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a measurement; see the top of the file"]
async fn a_keyed_camera_costs_this_much() {
    gst::init().unwrap();
    let (w, h) = canvas();
    let cfg: Config = toml::from_str(&format!(
        "[canvas]\nwidth = {w}\nheight = {h}\nfps = 30\nsample_rate = 48000\nchannels = 2\n\n[control]\nbind = \"127.0.0.1:0\"\n"
    ))
    .unwrap();
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).expect("build a mixer");
    mix.start().expect("start");
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    let presenter: String = match std::env::var("GMX_KEY_SOURCE").as_deref() {
        Ok("studio") => studio_png(w as u32, h as u32),
        _ => "test://smpte".to_string(),
    };
    for (id, uri) in [("set", "test://blue".to_string()), ("presenter", presenter.clone())] {
        let cfg = SourceConfig::bare(id, &uri);
        handle.request(|ack| Command::AddSource(Box::new(cfg), Some(ack))).await.expect("add");
    }
    let screen = if presenter.starts_with("test://") { (0, 255, 0) } else { (45, 175, 75) };
    let params: Params = toml::from_str(&format!(
        "method = \"custom\"\ntarget_r = {}\ntarget_g = {}\ntarget_b = {}",
        screen.0, screen.1, screen.2
    ))
    .unwrap();
    let key = vec![ItemFilter { type_id: std::env::var("GMX_KEY_TYPE").unwrap_or("chroma/filter".into()), name: None, params }];
    take(&handle, vec![placement("set", w, h, vec![]), placement("presenter", w, h, vec![])]).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let plain = measure(8).await;
    take(&handle, vec![placement("set", w, h, vec![]), placement("presenter", w, h, key)]).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let keyed = measure(8).await;
    handle.send(Command::Shutdown).ok();
    let _ = thread.join();
    println!("{w}x{h}30: no key {plain:.1} percent of a core, keyed {keyed:.1}, the key {:.1}", keyed - plain);
}

/// A green screen with a figure on it, as a PNG: a head and shoulders in the
/// middle, about a third of the picture.
fn studio_png(w: u32, h: u32) -> String {
    use gstreamer::prelude::*;
    let path = std::env::temp_dir().join(format!("gmx-key-studio-{w}x{h}.png"));
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for j in 0..h {
        for i in 0..w {
            let (x, y) = (i as f32 / w as f32 - 0.5, j as f32 / h as f32);
            let head = (x / 0.08).powi(2) + ((y - 0.3) / 0.14).powi(2) < 1.0;
            let body = (x / 0.3).powi(2) + ((y - 1.0) / 0.55).powi(2) < 1.0;
            rgb.extend(if head { [224, 172, 140] } else if body { [60, 64, 92] } else { [45, 175, 75] });
        }
    }
    let pipe = gst::parse::launch(&format!(
        "appsrc name=s caps=video/x-raw,format=RGB,width={w},height={h},framerate=1/1 ! videoconvert ! pngenc ! filesink location=\"{}\"",
        path.display()
    ))
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let src = pipe.by_name("s").unwrap().downcast::<gstreamer_app::AppSrc>().unwrap();
    pipe.set_state(gst::State::Playing).unwrap();
    src.push_buffer(gst::Buffer::from_mut_slice(rgb)).unwrap();
    src.end_of_stream().unwrap();
    pipe.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(5), &[gst::MessageType::Eos]);
    pipe.set_state(gst::State::Null).unwrap();
    path.to_string_lossy().into_owned()
}
