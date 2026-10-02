//! A plugin process killed from outside is noticed within a tick and brought
//! back at once, for a source the mixer holds and for a shared device.
//!
//! Before, nothing looked at the process. The plugin here is the shell plugin
//! the other sidecar tests use, and only the shell is killed: the
//! `gst-launch-1.0` it started keeps writing, which is the shape of a plugin
//! that died and left a helper holding its stdout. Nothing about the picture
//! changes, so the only thing that can notice is a wait on the process, and
//! the helper must be gone after the restart, or a death leaks a process.
//!
//! These need a shell and `gst-launch-1.0`, and skip where those are missing.

#![cfg(unix)]

mod shared;

use godwinmix_core::config::SourceConfig;
use godwinmix_core::plugin::loader;
use godwinmix_core::prelude::*;
use shared::exit::*;
use shared::*;
use std::time::{Duration, Instant};

async fn status(handle: &MixerHandle) -> MixerStatus {
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.send(Command::Status(tx)).ok();
    rx.await.expect("the mixer answers")
}

// The lock only keeps the two tests in this file apart: the plugin registry
// is one per process. Nothing else in the runtime waits on it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sidecar_whose_process_is_killed_is_restarted_within_a_second_or_two() {
    let _lock = exclusive();
    if skip() {
        return;
    }
    let _ = gstreamer::init();
    let root = temp("exit-plain");
    env_for(&root);
    install(&root, "exitbars", false);

    let mut cfg: Config = toml::from_str("").unwrap();
    (cfg.canvas.width, cfg.canvas.height, cfg.canvas.fps) = (640, 360, 30);
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).expect("a mixer");
    mix.start().expect("it starts");
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    let source: SourceConfig = toml::from_str("id = \"bars\"\ntype = \"exitbars/source\"").unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.send(Command::AddSource(Box::new(source), Some(tx))).ok();
    rx.await.unwrap().expect("the source is added");

    let pid = || loader::stats_for("bars").and_then(|s| s.pid);
    let live = |s: &MixerStatus| {
        s.sources.iter().any(|x| x.id.as_str() == "bars" && x.state == SourceState::Live)
    };
    let began = Instant::now();
    while !live(&status(&handle).await) && began.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let first = pid().expect("the plugin has a pid");

    kill_plugin(first);
    let killed = Instant::now();
    let mut second = None;
    while killed.elapsed() < Duration::from_secs(8) {
        second = pid().filter(|p| *p != first);
        if second.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let respawned = killed.elapsed();
    let second = second.expect("the plugin was started again");
    while !live(&status(&handle).await) && killed.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let back = killed.elapsed();
    println!("killed {first}: new process {second} after {respawned:?}, live again after {back:?}");
    let helper_gone = gone_within(first);

    handle.send(Command::Shutdown).ok();
    tokio::task::spawn_blocking(move || thread.join()).await.ok();
    loader::uninstall("exitbars").ok();
    assert!(respawned < Duration::from_secs(2), "the restart took {respawned:?}");
    assert!(back < Duration::from_secs(4), "live again only after {back:?}");
    assert!(helper_gone, "the dead plugin's helper outlived the restart");
    assert_eq!(opens(&root), 2);
}

#[test]
fn a_shared_device_whose_plugin_is_killed_is_opened_again_at_once() {
    let _lock = exclusive();
    if skip() {
        return;
    }
    let _ = gstreamer::init();
    let root = temp("exit-shared");
    env_for(&root);
    install(&root, "exitshare", true);

    let mut cam = open("exitshare/source", "cam", &[("device", "exit-test")]);
    assert!(until(Duration::from_secs(10), || cam.frames() > 10), "a picture at first");
    let first = cam.share()["plugin_pid"].as_u64().expect("the owner's plugin pid") as u32;
    kill_plugin(first);
    let killed = Instant::now();
    let reopened = until(Duration::from_secs(5), || {
        cam.share()["plugin_pid"].as_u64().is_some_and(|p| p as u32 != first)
    });
    let took = killed.elapsed();
    println!("the shared device was opened again {took:?} after its plugin was killed");
    let before = cam.frames();
    let flowing = until(Duration::from_secs(5), || cam.frames() > before + 30);
    let helper_gone = gone_within(first);
    cam.close();
    loader::uninstall("exitshare").ok();
    assert!(reopened && took < Duration::from_secs(1), "reopened {reopened}, after {took:?}");
    assert!(flowing, "the picture came back");
    assert!(helper_gone, "the dead plugin's helper outlived the reopen");
    assert_eq!(opens(&root), 2);
}
