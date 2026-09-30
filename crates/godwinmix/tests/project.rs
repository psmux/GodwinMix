//! `project.export` and `project.import` between two real cores.
//!
//! One mixer is set up the way a person would set it up: a setting in its
//! commented config file, two test cameras, a destination, a channel with a
//! key, and a scene that draws a camera. Its project is opened on a second,
//! fresh mixer, and the second has to end up with the same scenes, channels,
//! sources and outputs. Then the same file is merged into the first, which
//! already has all of it, and everything has to arrive renamed rather than
//! over the top of what is there.

use godwinmix::control::{call, methods, AppState};
use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::{self, Mixer};
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::Token;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const FILE: &str = "\
# The hall mixer. Written by hand; keep this line.
[canvas]
width = 320 # small, for the test
height = 180
fps = 30

[program]
video_bitrate_kbps = 900

[multiview]
enabled = false
";

struct Core {
    app: AppState,
    snapshots: Arc<Tracker>,
    registry: Registry<call::Call>,
    path: PathBuf,
}

async fn start(dir: &Path, text: &str) -> Core {
    let _ = gstreamer::init();
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("godwinmix.toml");
    std::fs::write(&path, format!("{text}\n[media]\ndir = \"{}\"\n", dir.join("media").display())).unwrap();
    let cfg = Config::load(&path).expect("the test config loads");
    let (mut mix, handle, cmd_rx, mut bus_rx) = Mixer::build(cfg.clone()).expect("building");
    mix.start().expect("starting the mixer");
    {
        let handle = handle.clone();
        tokio::spawn(async move {
            while let Some(ev) = bus_rx.recv().await {
                if handle.send(mixer::Command::Bus(ev)).is_err() {
                    return;
                }
            }
        });
    }
    let multiview = mix.multiview_handle();
    let preview = mix.preview_handle();
    let encoder = mix.encoder_handle();
    let _thread = mixer::spawn(mix, cmd_rx, handle.clone());
    let snapshots = Tracker::new(cfg.snapshot.clone(), multiview.clone(), handle.clone());
    let caps = godwinmix_core::caps::CanvasCaps::new(&cfg.canvas);
    let app = AppState::new(
        &cfg,
        godwinmix::control::Engine {
            mixer: handle.clone(),
            multiview,
            preview,
            encoder,
            library: Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone())),
            converter: Arc::new(godwinmix_core::convert::Converter::new(handle.clone(), 1, 1)),
            quit: Arc::new(tokio::sync::Notify::new()),
            scenes: godwinmix_core::scene::server::SceneServer::in_memory(caps.clone()),
            plugins: godwinmix_core::plugin::supervisor::Supervisor::new(caps, Default::default()),
        },
        false,
    );
    methods::config::configure(&cfg, &[]);
    Core { app, snapshots, registry: methods::registry(), path }
}

impl Core {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let token = Token { id: "admin".into(), ..Token::open() };
        call::dispatch(&self.registry, &self.app, &self.snapshots, &token, "test", method, params).await
    }

    async fn add_source(&self, id: &str) {
        let source: SourceConfig =
            toml::from_str(&format!("id = \"{id}\"\ntype = \"test/source\"\nuri = \"test://smpte\"\n")).unwrap();
        self.app
            .mixer
            .request(|ack| mixer::Command::AddSource(Box::new(source), Some(ack)))
            .await
            .expect("the source was added");
    }

    async fn ids(&self, what: &str) -> Vec<String> {
        let configs = self.app.mixer.configs().await.unwrap();
        let mut ids: Vec<String> = match what {
            "sources" => configs.sources.iter().map(|s| s.id.clone()).collect(),
            _ => configs.outputs.iter().map(|o| o.id.clone()).collect(),
        };
        ids.sort();
        ids
    }

    fn scene_names(&self) -> Vec<String> {
        self.app.scenes.document().scenes.iter().map(|s| s.name.clone()).collect()
    }

    fn channel_ids(&self) -> Vec<String> {
        self.app.channels.project_ids().into_iter().map(|(id, _)| id).collect()
    }
}

/// One secret store for the whole process, as a real core has: it is opened
/// once, by whichever test gets there first, so both must name the same one.
fn shared_home() {
    std::env::set_var("GODWINMIX_HOME", std::env::temp_dir().join(format!("gmx-project-home-{}", std::process::id())));
}

/// The first mixer, set up by hand.
async fn a_working_mixer(dir: &Path) -> Core {
    let core = start(dir, FILE).await;
    core.add_source("cam1").await;
    core.add_source("cam2").await;
    core.call("output.add", json!({"id": "hall", "uri": "rtmp://127.0.0.1:1/live/hallstreamkey"}))
        .await
        .expect("the destination was added");
    core.call("channel.add", json!({"name": "Sunday service"})).await.expect("the channel was made");
    core.call("scene.add", json!({"name": "Wide"})).await.unwrap();
    core.call("scene.item.add", json!({"scene": "Wide", "content": {"source": "cam1"}})).await.unwrap();
    std::fs::create_dir_all(dir.join("media")).unwrap();
    std::fs::write(dir.join("media/intro.mp4"), b"not really a video").unwrap();
    core
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_opened_on_a_fresh_mixer_brings_everything_and_a_merge_renames() {
    let root = std::env::temp_dir().join(format!("gmx-project-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    shared_home();
    let first = a_working_mixer(&root.join("first")).await;

    let file = first
        .call("project.export", json!({"name": "Hall", "include_secrets": true, "include_media": true, "page": {"theme": "dark"}}))
        .await
        .expect("the export");
    assert_eq!(file["format"], "godwinmix.project");
    assert_eq!(file["settings"]["program.video_bitrate_kbps"], 900);
    assert_eq!(file["media"][0]["name"], "intro.mp4");
    let secret = first.call("channel.key.reveal", json!({"id": "sunday-service", "key": "key-1"})).await.unwrap();
    assert_eq!(file["channels"][0]["secrets"]["keys"]["key-1"], secret["secret"]);

    let fresh = start(&root.join("fresh"), "[canvas]\nwidth = 320\nheight = 180\nfps = 30\n[multiview]\nenabled = false\n").await;
    let dry = fresh.call("project.import", json!({"file": file, "mode": "replace"})).await.expect("the dry run");
    assert_eq!(dry["dry_run"], true);
    assert!(fresh.ids("sources").await.is_empty(), "a dry run changes nothing");
    let parts: Vec<&str> = dry["changes"].as_array().unwrap().iter().filter_map(|c| c["part"].as_str()).collect();
    for part in ["setting", "source", "output", "channel", "scene", "media"] {
        assert!(parts.contains(&part), "the dry run names the {part}s: {dry}");
    }

    let done = fresh
        .call("project.import", json!({"file": file, "mode": "replace", "dry_run": false}))
        .await
        .expect("the import");
    assert!(done["failed"].as_array().unwrap().is_empty(), "{done}");
    assert_eq!(fresh.ids("sources").await, vec!["cam1", "cam2"]);
    assert_eq!(fresh.ids("outputs").await, vec!["hall"]);
    assert_eq!(fresh.scene_names(), first.scene_names());
    assert_eq!(fresh.channel_ids(), vec!["sunday-service"]);
    assert_eq!(done["page"]["theme"], "dark");
    assert!(std::fs::read(root.join("fresh/media/intro.mp4")).is_ok(), "the clip came across");
    let text = std::fs::read_to_string(&fresh.path).unwrap();
    assert!(text.contains("video_bitrate_kbps = 900"), "{text}");
    assert!(
        done["needs_restart"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().starts_with("program.video_bitrate_kbps")),
        "{done}"
    );
    let wide = fresh.app.scenes.document();
    let drawn = serde_json::to_string(&wide.scenes[0]).unwrap();
    assert!(drawn.contains("\"cam1\""), "the scene still draws its camera: {drawn}");

    // The same file merged into the mixer it came from: everything is taken,
    // so everything arrives beside it under a new name.
    let merged = first
        .call("project.import", json!({"file": file, "mode": "merge", "dry_run": false}))
        .await
        .expect("the merge");
    assert!(merged["failed"].as_array().unwrap().is_empty(), "{merged}");
    assert_eq!(first.ids("sources").await, vec!["cam1", "cam1-2", "cam2", "cam2-2"]);
    assert_eq!(first.ids("outputs").await, vec!["hall", "hall-2"]);
    assert_eq!(first.channel_ids(), vec!["sunday-service", "sunday-service-2"]);
    assert_eq!(first.scene_names().len(), 2, "{:?}", first.scene_names());
    let doc = first.app.scenes.document();
    assert_ne!(doc.scenes[0].id, doc.scenes[1].id, "a merged scene gets a new id");
    let second = serde_json::to_string(&doc.scenes[1]).unwrap();
    assert!(second.contains("\"cam1-2\""), "the merged scene draws the renamed camera: {second}");
    let renamed = merged["changes"].as_array().unwrap().iter().filter(|c| c["action"] == "rename").count();
    assert!(renamed >= 5, "{merged}");
    std::fs::remove_dir_all(&root).ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_keys_a_destination_waits_and_a_newer_file_is_refused() {
    let root = std::env::temp_dir().join(format!("gmx-project-keys-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    shared_home();
    let first = a_working_mixer(&root.join("first")).await;
    let file = first.call("project.export", json!({})).await.unwrap();
    let text = serde_json::to_string(&file).unwrap();
    assert!(!text.contains("hallstreamkey"), "no stream key without include_secrets");
    assert!(file["channels"][0].get("secrets").is_none());
    assert!(file["media"][0].get("data").is_none(), "clips by name and size only");

    let fresh = start(&root.join("fresh"), FILE).await;
    let done = fresh.call("project.import", json!({"file": text, "dry_run": false})).await.unwrap();
    assert!(fresh.ids("outputs").await.is_empty(), "a destination with no key is not started");
    let waiting = done["waiting"].as_array().unwrap();
    assert!(waiting.iter().any(|w| w.as_str().unwrap().starts_with("output hall")), "{done}");
    assert!(waiting.iter().any(|w| w.as_str().unwrap().contains("new key")), "{done}");
    assert!(waiting.iter().any(|w| w.as_str().unwrap().contains("intro.mp4")), "{done}");

    let mut newer = file.clone();
    newer["version"] = json!(2);
    let refused = fresh.call("project.import", json!({"file": newer})).await.unwrap_err();
    assert!(refused.message.contains("format 2"), "{}", refused.message);
    assert_eq!(refused.data["reason"], "newer_version");

    // New project: an empty file, replacing, leaves an empty mixer.
    let empty = json!({"format": "godwinmix.project", "version": 1, "name": "New project"});
    fresh.call("project.import", json!({"file": empty, "mode": "replace", "dry_run": false})).await.unwrap();
    assert!(fresh.ids("sources").await.is_empty());
    assert!(fresh.scene_names().is_empty());
    assert!(fresh.channel_ids().is_empty());
    std::fs::remove_dir_all(&root).ok();
}
