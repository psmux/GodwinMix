//! A clip transition's take does not hold the mixer loop while its clip
//! opens: the clip opens on a worker (`mixer::fx_open`). A file of its own,
//! so no other test's mixers share the process and the log.
//!
//! Judged by the mixer's own line for a command that held its loop for
//! 100 ms or more (`observe::laps`), not by how long the answer took to come
//! back, which on a loaded machine includes time the loop was not running.

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

#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Log {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_clip_transition_answers_before_its_clip_opens() {
    let log = Log::default();
    let writer = log.clone();
    let _ = tracing_subscriber::fmt().with_ansi(false).with_writer(move || writer.clone()).try_init();
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("a", "test://blue")).await;
    add(&handle, SourceConfig::bare("b", "test://red")).await;
    take(&handle, vec![full("a")]).await;
    settle(1_200).await;

    let media = scratch("fx-media-hold");
    let (m, dir) = fx::library::find(&media, "glitch").expect("the starter set is written on first read");
    let plan = Plan::of(&m, &dir, None).expect("a plan");
    let transition = TransitionSpec { duration_ms: plan.duration_ms, kind: Kind::Fx(Box::new(plan)), easing: Easing::default() };
    let scene = ProgramScene { name: "b".into(), placements: vec![full("b")] };
    let asked = Instant::now();
    handle
        .request(|ack| Command::TakeScene { scene: Box::new(scene), at_running_time_ms: None, duration_ms: None, transition: Some(transition), ack: Some(ack) })
        .await
        .expect("take");
    let take_ms = asked.elapsed().as_millis();
    settle(2_000).await;
    let end = frames.latest().unwrap().yuv(160, 90);
    stop(handle, thread);

    let text = String::from_utf8_lossy(&log.0.lock().unwrap()).into_owned();
    let held: Vec<&str> = text.lines().filter(|l| l.contains("held the mixer loop") && l.contains("program.take")).collect();
    println!("take answered in {take_ms} ms");
    assert!(held.is_empty(), "a take held the loop: {held:#?}");
    assert!(near(end, (63, 102, 240)), "the new scene is on after the stinger: {end:?}");
}
