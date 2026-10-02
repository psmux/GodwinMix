//! One real mixer with the whole method table in front of it, started from a
//! config file in a scratch folder so the feeds file is written beside it.

use godwinmix::control::{call, methods, AppState};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::Token;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const FILE: &str = "[canvas]\nwidth = 320\nheight = 180\nfps = 30\n\n[multiview]\nenabled = false\n";

pub struct Core {
    pub app: AppState,
    pub snapshots: Arc<Tracker>,
    registry: Registry<call::Call>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub path: PathBuf,
}

/// The secret store under a scratch home, never the operator's own.
fn scratch_home() {
    static HOME: std::sync::Once = std::sync::Once::new();
    HOME.call_once(|| {
        let home = std::env::temp_dir().join(format!("gmx-feeds-home-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("GODWINMIX_HOME", home);
    });
}

impl Core {
    pub async fn start(tag: &str) -> Core {
        scratch_home();
        let _ = gstreamer::init();
        let dir = std::env::temp_dir().join(format!("gmx-feeds-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("godwinmix.toml");
        std::fs::write(&path, FILE).unwrap();
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
        let (multiview, preview, encoder) = (mix.multiview_handle(), mix.preview_handle(), mix.encoder_handle());
        let thread = mixer::spawn(mix, cmd_rx, handle.clone());
        let snapshots = Tracker::new(cfg.snapshot.clone(), multiview.clone(), handle.clone());
        let caps = godwinmix_core::caps::CanvasCaps::new(&cfg.canvas);
        let engine = godwinmix::control::Engine {
            mixer: handle.clone(),
            multiview,
            preview,
            encoder,
            library: Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone())),
            converter: Arc::new(godwinmix_core::convert::Converter::new(handle.clone(), 1, 1)),
            quit: Arc::new(tokio::sync::Notify::new()),
            scenes: godwinmix_core::scene::server::SceneServer::in_memory(caps.clone()),
            plugins: godwinmix_core::plugin::supervisor::Supervisor::new(caps, Default::default()),
        };
        let app = AppState::new(&cfg, engine, false);
        Core { app, snapshots, registry: methods::registry(), thread: Some(thread), path }
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let token = Token { id: "desk".into(), ..Token::open() };
        call::dispatch(&self.registry, &self.app, &self.snapshots, &token, "test", method, params).await
    }

    pub async fn ok(&self, method: &str, params: Value) -> Value {
        self.call(method, params.clone()).await.unwrap_or_else(|e| panic!("{method} {params} was refused: {e:?}"))
    }

    /// A text source, live.
    pub async fn text(&self, id: &str, words: &str) {
        self.ok("source.add", json!({ "id": id, "uri": format!("text:{words}") })).await;
        self.until(&format!("{id} goes live"), || async {
            self.ok("source.get", json!({ "id": id })).await["state"] == "live"
        })
        .await;
    }

    /// A source's param, as the mixer holds it.
    pub async fn param(&self, id: &str, key: &str) -> Value {
        let configs = self.app.mixer.configs().await.unwrap();
        let source = configs.sources.iter().find(|s| s.id == id).expect("the source exists");
        source.params.get(key).map(|v| serde_json::to_value(v).unwrap()).unwrap_or(Value::Null)
    }

    pub async fn feed(&self, id: &str) -> Value {
        let list = self.ok("feed.list", json!({})).await;
        list["feeds"].as_array().unwrap().iter().find(|f| f["id"] == id).cloned().unwrap_or(Value::Null)
    }

    pub async fn binding(&self, id: &str) -> Value {
        let list = self.ok("feed.list", json!({})).await;
        list["bindings"].as_array().unwrap().iter().find(|b| b["id"] == id).cloned().unwrap_or(Value::Null)
    }

    /// Wait for `cond`, for at most ten seconds.
    pub async fn until<F, Fut>(&self, what: &str, cond: F)
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !cond().await {
            assert!(Instant::now() < deadline, "waited ten seconds and {what} did not happen");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.app.mixer.send(mixer::Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Some(dir) = self.path.parent() {
            std::fs::remove_dir_all(dir).ok();
        }
    }
}
