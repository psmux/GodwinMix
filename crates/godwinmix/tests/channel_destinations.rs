//! `channel.destination.*` on a real core with the real channel registry:
//! a destination is kept with its channel, its address and key are sealed
//! rather than written beside the channel, every change is announced, and a
//! core started again from the same files has it back.

use godwinmix::control::{call, methods, AppState};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{Mixer, MixerHandle};
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::Token;
use godwinmix_protocol::types::Event;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

const FILE: &str = "\
[canvas]
width = 320
height = 180
fps = 30

[multiview]
enabled = false
";

struct Core {
    app: AppState,
    snapshots: Arc<Tracker>,
    registry: Registry<call::Call>,
    mixer: MixerHandle,
}

fn app(cfg: &Config, handle: &MixerHandle, mix: &Mixer) -> AppState {
    AppState::new(
        cfg,
        godwinmix::control::Engine {
            mixer: handle.clone(),
            multiview: mix.multiview_handle(),
            preview: mix.preview_handle(),
            encoder: mix.encoder_handle(),
            library: Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone())),
            converter: Arc::new(godwinmix_core::convert::Converter::new(handle.clone(), 1, 1)),
            quit: Arc::new(tokio::sync::Notify::new()),
            scenes: godwinmix_core::scene::server::SceneServer::in_memory(
                godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
            ),
            plugins: godwinmix_core::plugin::supervisor::Supervisor::new(
                godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
                Default::default(),
            ),
        },
        false,
    )
}

/// A core over the config at `path`. The mixer is built but not started:
/// channels need nothing from a running pipeline.
fn start(path: &PathBuf) -> Core {
    let _ = gstreamer::init();
    let cfg = Config::load(path).expect("the test config loads");
    let (mix, handle, _cmd_rx, _bus_rx) = Mixer::build(cfg.clone()).expect("building");
    let snapshots = Tracker::new(cfg.snapshot.clone(), mix.multiview_handle(), handle.clone());
    let app = app(&cfg, &handle, &mix);
    Core { app, snapshots, registry: methods::registry(), mixer: handle }
}

impl Core {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let token = Token { id: "admin".into(), ..Token::open() };
        call::dispatch(&self.registry, &self.app, &self.snapshots, &token, "test", method, params).await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_is_kept_sealed_announced_and_back_after_a_restart() {
    let dir = std::env::temp_dir().join(format!("gmx-channel-destinations-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("GODWINMIX_HOME", dir.join("home"));
    let path = dir.join("godwinmix.toml");
    std::fs::write(&path, FILE).unwrap();

    let core = start(&path);
    let mut events = core.mixer.subscribe();
    core.call("channel.add", json!({"name": "Sunday service"})).await.unwrap();
    let channel = core
        .call(
            "channel.destination.add",
            json!({"id": "sunday-service", "platform": "custom", "label": "Hall",
                   "server": "rtmp://10.0.0.9:1935/live", "key": "hall-secret-key"}),
        )
        .await
        .unwrap();
    let d = &channel["destinations"][0];
    assert_eq!(d["id"], "hall");
    assert_eq!(d["has_key"], true);
    assert_eq!(d["uri_host"], "rtmp://10.0.0.9:1935");
    assert_eq!(d["state"], "waiting", "on, and nothing has reported yet");
    assert!(!channel.to_string().contains("hall-secret-key"));

    let announced = loop {
        match events.recv().await.unwrap().event {
            Event::ChannelChanged { channel } if !channel.destinations.is_empty() => break channel,
            _ => continue,
        }
    };
    assert_eq!(announced.destinations[0].id, "hall");

    let off = core
        .call("channel.destination.set", json!({"id": "sunday-service", "destination": "hall", "enabled": false}))
        .await
        .unwrap();
    assert_eq!(off["destinations"][0]["state"], "off");
    assert_eq!(off["destinations"][0]["has_key"], true, "a key left out is kept");

    let on_disk = std::fs::read_to_string(dir.join("godwinmix.runtime.channels.toml")).unwrap();
    assert!(on_disk.contains("uri_host = \"rtmp://10.0.0.9:1935\""), "{on_disk}");
    assert!(!on_disk.contains("hall-secret-key") && !on_disk.contains("/live"), "{on_disk}");
    let sealed = std::fs::read_to_string(dir.join("home/secrets/store.json")).unwrap();
    assert!(sealed.contains("channel.sunday-service.destination.hall"), "{sealed}");
    assert!(!sealed.contains("hall-secret-key"), "sealed, not written: {sealed}");

    drop(core);
    let again = start(&path);
    let back = again.call("channel.get", json!({"id": "sunday-service"})).await.unwrap();
    assert_eq!(back["destinations"][0]["label"], "Hall");
    assert_eq!(back["destinations"][0]["has_key"], true);
    let moved = again
        .call("channel.destination.set", json!({"id": "sunday-service", "destination": "hall", "label": "Hall 2"}))
        .await
        .unwrap();
    assert_eq!(moved["destinations"][0]["uri_host"], "rtmp://10.0.0.9:1935", "the sealed address came back");

    let gone = again.call("channel.remove", json!({"id": "sunday-service"})).await.unwrap();
    assert_eq!(gone["removed"], "sunday-service");
    let sealed = std::fs::read_to_string(dir.join("home/secrets/store.json")).unwrap();
    assert!(!sealed.contains("sunday-service"), "a channel removed forgets its destinations: {sealed}");
    std::fs::remove_dir_all(&dir).ok();
}
