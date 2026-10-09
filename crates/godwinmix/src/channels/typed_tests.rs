//! A channel moved from Livebox: its address kept with its capitals, its
//! password kept as the key, on a real registry over a real channels file
//! and secret store.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use godwinmix_core::caps::CanvasCaps;
use godwinmix_core::config::Config;
use godwinmix_core::mixer::Mixer;
use godwinmix_core::plugin::supervisor::Supervisor;
use godwinmix_core::scene::server::SceneServer;
use godwinmix_core::secrets::Secrets;
use godwinmix_protocol::channels::{ChannelAddRequest, ChannelKeyAddRequest, ChannelKeyRevealRequest};
use serde_json::json;

use super::super::{Channels, Ports};

const FILE: &str = "[canvas]\nwidth = 320\nheight = 180\nfps = 30\n\n[multiview]\nenabled = false\n";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-typed-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn open(dir: &Path) -> Arc<Channels> {
    let _ = gstreamer::init();
    let path = dir.join("godwinmix.toml");
    std::fs::write(&path, FILE).unwrap();
    let cfg = Config::load(&path).unwrap();
    let (_mix, handle, _cmd, _bus) = Mixer::build(cfg.clone()).unwrap();
    let caps = CanvasCaps::new(&cfg.canvas);
    let secrets: &'static Secrets = Box::leak(Box::new(Secrets::open(&dir.join("secrets")).unwrap()));
    Channels::open(
        Some(Config::runtime_store_path(&path)),
        Ports::default(),
        Supervisor::new(caps.clone(), Default::default()),
        handle.clone(),
        Arc::new(crate::channels::target::Local { mixer: handle, scenes: SceneServer::in_memory(caps) }),
        secrets,
    )
}

fn add(value: serde_json::Value) -> ChannelAddRequest {
    serde_json::from_value(value).unwrap()
}

#[tokio::test]
async fn a_livebox_channel_keeps_its_address_and_password() {
    let dir = scratch("keep");
    let channels = open(&dir);
    let made = channels.add(add(json!({"name": "Church", "app": "Church", "secret": "Sunday-2024"}))).unwrap();
    assert_eq!((made.channel.id.as_str(), made.channel.app.as_str()), ("church", "Church"));
    assert_eq!(made.key.secret, "Sunday-2024");
    assert!(made.channel.keys[0].imported, "a typed secret is marked so");
    assert_eq!(made.channel.keys[0].hint, "2024");
    assert!(made.channel.publish.server.ends_with("/Church"), "{}", made.channel.publish.server);

    let back = channels
        .key_reveal(ChannelKeyRevealRequest { id: "church".into(), key: made.key.id.clone() }, "test")
        .unwrap();
    assert_eq!(back.secret, "Sunday-2024", "sealed the way a made key is");
    let file = std::fs::read_to_string(dir.join("godwinmix.runtime.channels.toml")).unwrap();
    assert!(!file.contains("Sunday-2024"), "the secret is never in the channels file: {file}");
    assert!(file.contains("imported = true"), "{file}");

    // A made key is not marked.
    let made_here = channels.key_add(ChannelKeyAddRequest { id: "church".into(), label: None, secret: None }).unwrap();
    let listed = channels.get("church").unwrap();
    assert!(!listed.keys.iter().find(|k| k.id == made_here.key.id).unwrap().imported);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn an_address_that_differs_only_in_case_is_taken() {
    let dir = scratch("case");
    let channels = open(&dir);
    channels.add(add(json!({"name": "Church", "app": "Church"}))).unwrap();
    let err = channels.add(add(json!({"name": "church again", "app": "church"}))).unwrap_err();
    assert_eq!(err.data["field"], "app");
    assert_eq!(err.data["channel"], "church", "names the channel that has it: {}", err.message);
    assert_eq!(err.data["app"], "Church");
    assert!(err.message.contains("whatever case"), "{}", err.message);

    // A space is kept, and shown as an encoder has to send it.
    let hall = channels.add(add(json!({"name": "Youth Hall", "app": "Youth Hall"}))).unwrap().channel;
    assert_eq!((hall.id.as_str(), hall.app.as_str()), ("youth-hall", "Youth Hall"));
    assert!(hall.publish.server.ends_with("/Youth%20Hall"), "{}", hall.publish.server);
    assert!(hall.publish.example.ends_with("/Youth%20Hall/main?psk=<key>"), "{}", hall.publish.example);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_bad_secret_makes_no_channel_and_a_repeated_one_adds_no_key() {
    let dir = scratch("bad");
    let channels = open(&dir);
    let err = channels.add(add(json!({"name": "Hall", "app": "Hall", "secret": "abc"}))).unwrap_err();
    assert_eq!(err.data["min_len"], 6);
    assert!(channels.list().channels.is_empty(), "a refused secret leaves nothing half made");

    channels.add(add(json!({"name": "Hall", "secret": "hall-pass"}))).unwrap();
    let twice = ChannelKeyAddRequest { id: "hall".into(), label: Some("Livebox".into()), secret: Some("hall-pass".into()) };
    let err = channels.key_add(twice).unwrap_err();
    assert_eq!(err.data["key"], "key-1");
    assert!(!err.message.contains("hall-pass"));
    let other = ChannelKeyAddRequest { id: "hall".into(), label: Some("Livebox".into()), secret: Some("other pass".into()) };
    let key = channels.key_add(other).unwrap().key;
    assert_eq!((key.id.as_str(), key.secret.as_str()), ("livebox", "other pass"));
    assert_eq!(channels.get("hall").unwrap().keys.len(), 2);
    std::fs::remove_dir_all(&dir).ok();
}
