//! One real mixer with the whole method table in front of it, started from a
//! config file in a scratch folder, with its media folder beside it.

use godwinmix::control::{call, methods, AppState};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::Token;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

const FILE: &str = "[canvas]\nwidth = 320\nheight = 180\nfps = 30\n\n[multiview]\nenabled = false\n\n[[sources]]\nid = \"cam1\"\nuri = \"test://ball\"\n";

pub struct Core {
    pub app: AppState,
    snapshots: Arc<Tracker>,
    registry: Registry<call::Call>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub path: PathBuf,
}

impl Core {
    pub async fn start(tag: &str) -> Core {
        let _ = gstreamer::init();
        let dir = std::env::temp_dir().join(format!("gmx-replay-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("godwinmix.toml");
        let media = dir.join("media").display().to_string();
        std::fs::write(&path, format!("{FILE}\n[media]\ndir = {media:?}\n")).unwrap();
        let cfg = Config::load(&path).expect("the test config loads");
        let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building");
        mix.start().expect("starting the mixer");
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
            scenes: godwinmix_core::scene::server::SceneServer::in_memory(caps),
            plugins: godwinmix_core::plugin::supervisor::Supervisor::detached(),
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

    /// The refusal, as a client reads it.
    pub async fn refused(&self, method: &str, params: Value) -> RpcError {
        match self.call(method, params.clone()).await {
            Ok(answer) => panic!("{method} {params} was taken: {answer}"),
            Err(e) => e,
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
