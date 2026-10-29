//! Replay on a running mixer: nothing built while it is off, the rings
//! filling, an event played back on the right frames at full and half speed,
//! the angle changed while it plays, and the programme on time throughout.

use godwinmix_core::observe::introspect::pipeline;
use godwinmix_core::config::SourceConfig;
use godwinmix_core::prelude::*;
use godwinmix_core::replay::ReplayCall;
use godwinmix_protocol::replay::{MarkRequest, ReplayConfig};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::time::Duration;

mod playback;

pub const SEC: u64 = 1_000_000_000;

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub struct Rig {
    pub handle: MixerHandle,
    thread: std::thread::JoinHandle<()>,
    // Pipelines are found by name and the frame counter is the process's, so
    // one mixer at a time.
    _serial: tokio::sync::MutexGuard<'static, ()>,
}

pub async fn rig() -> Rig {
    let serial = SERIAL.lock().await;
    gst::init().unwrap();
    let mut cfg = Config::from_toml("", "replay test").unwrap();
    cfg.canvas.width = 320;
    cfg.canvas.height = 180;
    let (mut mix, handle, cmd_rx, mut bus_rx) = Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let bus_handle = handle.clone();
    tokio::spawn(async move {
        while let Some(event) = bus_rx.recv().await {
            if bus_handle.send(Command::Bus(event)).is_err() {
                break;
            }
        }
    });
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    for id in ["cam1", "cam2"] {
        let src = SourceConfig::bare(id, "test://ball");
        handle.request(|ack| Command::AddSource(Box::new(src), Some(ack))).await.unwrap();
    }
    handle.request(|ack| Command::Take { source: Some("cam1".into()), at_running_time_ms: None, ack: Some(ack) }).await.unwrap();
    Rig { handle, thread, _serial: serial }
}

impl Rig {
    pub async fn stop(self) {
        self.handle.send(Command::Shutdown).unwrap();
        tokio::task::spawn_blocking(move || self.thread.join()).await.unwrap().unwrap();
    }

    pub async fn call(&self, call: ReplayCall) -> godwinmix_core::mixer::ReplayAnswer {
        self.handle.replay(call).await.unwrap()
    }
}

/// Every element name in the programme and both camera pipelines.
fn names() -> Vec<String> {
    let mut all = Vec::new();
    for p in ["program", "input-cam1", "input-cam2"] {
        let Some(p) = pipeline(p) else { continue };
        let mut it = p.iterate_recurse();
        while let Ok(Some(el)) = it.next() {
            all.push(el.name().to_string());
        }
    }
    all.sort();
    all
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn replay_off_builds_nothing_and_off_again_takes_everything_out() {
    let rig = rig().await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let before = names();
    assert!(!before.iter().any(|n| n.starts_with("replay-")), "nothing of replay while it is off");
    let (status, _) = rig.call(ReplayCall::Get).await.unwrap();
    assert!(!status.on && status.angles.is_empty());

    let cfg = ReplayConfig { seconds: 3, angles: vec!["cam1".into(), "programme".into()], bitrate_kbps: 0 };
    rig.call(ReplayCall::Start(cfg)).await.unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(names().iter().any(|n| n.starts_with("replay-cam1")), "cam1's branch is on its tee");
    let (status, _) = rig.call(ReplayCall::Get).await.unwrap();
    for angle in &status.angles {
        assert!(angle.held_secs >= 2.9 && angle.held_secs <= 6.0, "{} holds {}s for a depth of 3", angle.id, angle.held_secs);
        assert!(angle.bytes > 0 && angle.waiting.is_none(), "{angle:?}");
    }
    assert_eq!(status.angles.iter().find(|a| a.id == "programme").unwrap().encoder, "shared");

    rig.call(ReplayCall::Stop).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(names(), before, "off again, every pipeline is what it was");
    let shared = rig.handle.replay_shared();
    assert!(shared.ring("cam1").is_none(), "and the rings are freed");
    rig.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_mark_with_replay_off_says_how_to_turn_it_on() {
    let rig = rig().await;
    let refused = rig.call(ReplayCall::Mark(MarkRequest { seconds: Some(3.0), ..Default::default() })).await.unwrap_err();
    assert!(refused.to_string().contains("replay.start"), "{refused}");
    let bad = ReplayConfig { seconds: 4000, angles: vec!["cam1".into()], bitrate_kbps: 0 };
    let refused = rig.call(ReplayCall::Start(bad)).await.unwrap_err();
    assert!(refused.to_string().contains("1 to 1800"), "{refused}");
    rig.stop().await;
}
