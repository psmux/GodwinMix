//! Transitions and effects from the fx library on a running mixer: an alpha
//! stinger, a Screen overlay, a luma matte, a shader with its software
//! version, and an effect fired over a moving picture. Real GStreamer, the
//! starter set's own files, a 320x180 programme read back frame by frame.

use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::fx::{self, Plan};
use godwinmix_core::mixer::slots::{Placement, Sizing};
use godwinmix_core::mixer::transition::{Easing, Kind, TransitionSpec};
use godwinmix_core::mixer::{self, Command, Mixer, MixerHandle, ProgramScene};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[path = "overlay/support.rs"]
mod support;
use support::*;

const BLUE: (u8, u8, u8) = (32, 240, 118);
const RED: (u8, u8, u8) = (63, 102, 240);

/// The starter item `name`, as a take would have it.
fn plan(name: &str) -> Plan {
    let media = scratch(&format!("fx-media-{name}"));
    let (m, dir) = fx::library::find(&media, name).expect("the starter set is written on first read");
    Plan::of(&m, &dir, None).expect("a plan")
}

async fn take_with(handle: &MixerHandle, source: &str, plan: Option<Plan>) {
    let transition = plan.map(|p| TransitionSpec { duration_ms: p.duration_ms, kind: Kind::Fx(Box::new(p)), easing: Easing::default() });
    let scene = ProgramScene { name: source.into(), placements: vec![full(source)] };
    handle
        .request(|ack| Command::TakeScene { scene: Box::new(scene), at_running_time_ms: None, duration_ms: None, transition, ack: Some(ack) })
        .await
        .expect("take");
}

/// The programme at `points`, every 15 ms for `ms`.
async fn watch(frames: &Frames, points: &[(usize, usize)], ms: u64) -> Vec<Vec<(u8, u8, u8)>> {
    let until = Instant::now() + Duration::from_millis(ms);
    let mut seen = Vec::new();
    while Instant::now() < until {
        if let Some(f) = frames.latest() {
            seen.push(points.iter().map(|(x, y)| f.yuv(*x, *y)).collect());
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    seen
}

/// Two plain sources, blue on air.
async fn blue_then_red() -> (MixerHandle, Frames, std::thread::JoinHandle<()>) {
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("a", "test://blue")).await;
    add(&handle, SourceConfig::bare("b", "test://red")).await;
    take_with(&handle, "a", None).await;
    settle(1_200).await;
    (handle, frames, thread)
}

fn neither(p: (u8, u8, u8)) -> bool {
    !near(p, BLUE) && !near(p, RED)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_alpha_stinger_covers_the_cut_and_leaves_the_new_scene() {
    let (handle, frames, thread) = blue_then_red().await;
    take_with(&handle, "b", Some(plan("glitch"))).await;
    let seen = watch(&frames, &[(160, 90), (20, 20), (300, 160)], 1_400).await;
    settle(300).await;
    let end = frames.latest().unwrap().yuv(160, 90);
    let worst = frames.worst_interval();
    stop(handle, thread);
    let covered = seen.iter().filter(|s| s.iter().all(|p| neither(*p))).count();
    assert!(covered > 0, "the glitch never covered the picture: {seen:?}");
    assert!(near(end, RED), "the new scene is on after the stinger: {end:?}");
    assert!(worst < 100.0, "the programme kept its frames: worst interval {worst} ms");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_screen_overlay_lightens_the_picture_and_cuts_under_its_brightest_frame() {
    let (handle, frames, thread) = blue_then_red().await;
    take_with(&handle, "b", Some(plan("light-leak"))).await;
    let seen = watch(&frames, &[(160, 90)], 2_200).await;
    settle(300).await;
    let end = frames.latest().unwrap().yuv(160, 90);
    stop(handle, thread);
    let brightest = seen.iter().map(|s| s[0].0).max().unwrap_or(0);
    println!("luma through the leak: {:?}", seen.iter().map(|s| s[0].0).collect::<Vec<_>>());
    assert!(brightest > 150, "screen should lift the picture well above either source, peaked at {brightest}");
    assert!(near(end, RED), "the new scene is on after the leak: {end:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_iris_matte_shows_the_new_scene_in_the_middle_first() {
    let (handle, frames, thread) = blue_then_red().await;
    take_with(&handle, "b", Some(plan("iris"))).await;
    let seen = watch(&frames, &[(160, 90), (4, 4)], 1_100).await;
    settle(300).await;
    let end = frames.latest().unwrap().yuv(4, 4);
    stop(handle, thread);
    let opening = seen.iter().any(|s| near(s[0], RED) && near(s[1], BLUE));
    assert!(opening, "half way the middle is new and the corner old: {seen:?}");
    assert!(near(end, RED), "and at the end the corner is new too: {end:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_shader_transition_runs_and_lands_on_the_new_scene() {
    for name in ["glitch-slice", "ripple"] {
        let (handle, frames, thread) = blue_then_red().await;
        take_with(&handle, "b", Some(plan(name))).await;
        let seen = watch(&frames, &[(20, 20), (160, 90), (300, 160)], 1_300).await;
        settle(300).await;
        let end = frames.latest().unwrap().yuv(160, 90);
        let worst = frames.worst_interval();
        stop(handle, thread);
        let mixed = seen.iter().any(|s| s.iter().any(|p| near(*p, RED)) && s.iter().any(|p| near(*p, BLUE)) || s.iter().any(|p| neither(*p)));
        println!("{name}: runs {} here, {} frames looked at, worst interval {worst:.1} ms", fx::shader::runs(name), seen.len());
        assert!(mixed, "{name}: the old and new scenes were never on screen together: {seen:?}");
        assert!(near(end, RED), "{name}: the new scene is on after it: {end:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_effect_fired_over_a_moving_source_plays_and_goes() {
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("ball", "test://ball")).await;
    take_with(&handle, "ball", None).await;
    settle(1_000).await;
    let p = plan("film-burn");
    handle.request(|ack| Command::FireFx { plan: Box::new(p), opacity: 1.0, ack: Some(ack) }).await.expect("fire");
    let seen = watch(&frames, &[(20, 90)], 1_600).await;
    settle(2_500).await;
    let after = frames.latest().unwrap().yuv(20, 90);
    let worst = frames.worst_interval();
    stop(handle, thread);
    let burnt = seen.iter().any(|s| s[0].0 > 200);
    assert!(burnt, "the burn should white out the left edge: {seen:?}");
    assert!(after.0 < 60, "and be gone once its clip ends: {after:?}");
    assert!(worst < 100.0, "the programme kept its frames: worst interval {worst} ms");
}
