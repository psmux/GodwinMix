//! `channel.key.reveal` on a real core: an admin reads a key back, a read
//! token is refused, a list still carries only the hint, and a key sealed by
//! a core from before the method existed is read back the same way.

use godwinmix::control::{call, methods, AppState};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{Mixer, MixerHandle};
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::{Scope, Token};
use serde_json::{json, Value};
use std::path::Path;
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
fn start(path: &Path) -> Core {
    let _ = gstreamer::init();
    let cfg = Config::load(path).expect("the test config loads");
    let (mix, handle, _cmd_rx, _bus_rx) = Mixer::build(cfg.clone()).expect("building");
    let snapshots = Tracker::new(cfg.snapshot.clone(), mix.multiview_handle(), handle.clone());
    let app = app(&cfg, &handle, &mix);
    Core { app, snapshots, registry: methods::registry() }
}

impl Core {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.call_as(Token { id: "admin".into(), ..Token::open() }, method, params).await
    }

    async fn call_as(&self, token: Token, method: &str, params: Value) -> Result<Value, RpcError> {
        call::dispatch(&self.registry, &self.app, &self.snapshots, &token, "test", method, params).await
    }
}

/// A channel as a core from before `channel.key.reveal` left it: the record
/// beside the config, the key sealed under `channel.<id>`.
fn an_old_channel(dir: &Path) {
    std::fs::write(
        dir.join("godwinmix.runtime.channels.toml"),
        "[[channels]]\nid = \"youth\"\nname = \"Youth\"\napp = \"youth\"\n\n\
         [[channels.keys]]\nid = \"key-1\"\nlabel = \"Key 1\"\n\
         created = \"2026-09-01T10:00:00Z\"\nhint = \"wxyz\"\n",
    )
    .unwrap();
    let store = godwinmix_core::secrets::Secrets::open(&dir.join("home/secrets")).unwrap();
    store.set("channel.youth", "key-1", "an-old-key-made-last-wxyz").unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_admin_reads_a_key_back_and_a_read_token_never_sees_one() {
    let dir = std::env::temp_dir().join(format!("gmx-channel-keys-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("GODWINMIX_HOME", dir.join("home"));
    let path = dir.join("godwinmix.toml");
    std::fs::write(&path, FILE).unwrap();
    an_old_channel(&dir);

    let core = start(&path);
    let old = core.call("channel.key.reveal", json!({"id": "youth", "key": "key-1"})).await.unwrap();
    assert_eq!(old["secret"], "an-old-key-made-last-wxyz", "a key made before the method is read back");

    let added = core.call("channel.add", json!({"name": "Sunday service"})).await.unwrap();
    let made = added["key"]["secret"].as_str().unwrap().to_string();
    let again = core.call("channel.key.reveal", json!({"id": "sunday-service", "key": "key-1"})).await.unwrap();
    assert_eq!(again["secret"], made.as_str());
    assert_eq!(again.as_object().unwrap().len(), 1, "the secret and nothing else: {again}");

    let reader = Token { id: "viewer".into(), scopes: vec![Scope::Read], ..Token::open() };
    let refused = core
        .call_as(reader.clone(), "channel.key.reveal", json!({"id": "sunday-service", "key": "key-1"}))
        .await
        .unwrap_err();
    assert!(!refused.message.contains(&made) && !refused.data.to_string().contains(&made));
    assert!(refused.message.contains("admin"), "{}", refused.message);
    let listed = core.call_as(reader, "channel.list", json!({})).await.unwrap();
    assert!(!listed.to_string().contains(&made), "a list carries the hint only: {listed}");
    assert!(!listed.to_string().contains("an-old-key-made-last-wxyz"));

    let unknown = core.call("channel.key.reveal", json!({"id": "sunday-service", "key": "key-9"})).await.unwrap_err();
    assert!(unknown.message.contains("key-1"), "names the keys there are: {}", unknown.message);
    core.call("channel.key.remove", json!({"id": "sunday-service", "key": "key-1"})).await.unwrap();
    let gone = core.call("channel.key.reveal", json!({"id": "sunday-service", "key": "key-1"})).await;
    assert!(gone.is_err(), "a key taken back is not read back");
    std::fs::remove_dir_all(&dir).ok();
}
