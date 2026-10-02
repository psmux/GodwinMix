//! A whole core for the acceptance tests: pipelines running, two colour bar
//! sources live, and the method table in front of them. Shared by `live.rs`
//! and `templates.rs`, each of which uses some of it.
#![allow(dead_code)]

use super::*;

/// A whole core: pipelines running, two sources live, the method table in
/// front of them.
pub struct Core {
    pub app: AppState,
    pub snapshots: Arc<Tracker>,
    pub registry: Registry<call::Call>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Core {
    pub async fn start(safety: godwinmix_core::safety::SafetyConfig) -> Core {
        Core::start_with(safety, false).await
    }

    pub async fn start_with(
        safety: godwinmix_core::safety::SafetyConfig,
        rehearsal: bool,
    ) -> Core {
        Core::start_configured(safety, rehearsal, |_| {}).await
    }

    /// The same, with the config changed by `edit` before anything is built.
    pub async fn start_configured(
        safety: godwinmix_core::safety::SafetyConfig,
        rehearsal: bool,
        edit: impl FnOnce(&mut Config),
    ) -> Core {
        let _ = gstreamer::init();
        let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
        cfg.canvas.width = 320;
        cfg.canvas.height = 180;
        cfg.canvas.fps = 30;
        cfg.multiview.enabled = false;
        cfg.safety = safety;
        edit(&mut cfg);

        let (mut mix, handle, cmd_rx, mut bus_rx) =
            Mixer::build(cfg.clone()).expect("building the mixer");
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
        let thread = mixer::spawn(mix, cmd_rx, handle.clone());
        let snapshots =
            Tracker::new(cfg.snapshot.clone(), multiview.clone(), handle.clone());

        let library = Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone()));
        let converter = Arc::new(godwinmix_core::convert::Converter::new(
            handle.clone(),
            library.cfg().convert_threads,
            library.cfg().probe_timeout_secs,
        ));
        let app = AppState::new(
            &cfg,
            godwinmix::control::Engine {
                mixer: handle.clone(),
                multiview,
                preview,
                encoder,
                library,
                converter,
                quit: Arc::new(tokio::sync::Notify::new()),
                // In memory: a live test writes no scene collection to disk.
                scenes: godwinmix_core::scene::server::SceneServer::in_memory(
                    godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
                ),
                // No plugins are installed in a live test, so the supervisor
                // has nothing to run; it is here because the control plane
                // asks it what transitions exist.
                plugins: godwinmix_core::plugin::supervisor::Supervisor::new(
                    godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
                    Default::default(),
                ),
            },
            rehearsal,
        );
        let core = Core {
            snapshots,
            registry: methods::registry(),
            app,
            thread: Some(thread),
        };
        godwinmix::control::spawn_background(core.app.clone());
        for id in ["cam1", "cam2"] {
            core.add_source(id).await;
        }
        core.wait_live("cam1").await;
        core.wait_live("cam2").await;
        core
    }

    pub async fn add_source(&self, id: &str) {
        let source: SourceConfig = toml::from_str(&format!(
            "id = \"{id}\"\ntype = \"test/source\"\nuri = \"test://smpte\"\n"
        ))
        .expect("a valid source document");
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.app
            .mixer
            .send(mixer::Command::AddSource(Box::new(source), Some(tx)))
            .expect("the mixer takes the source");
        rx.await.expect("the mixer answered").expect("the source was added");
    }

    /// Colour bars come up in well under a second; this waits rather than
    /// sleeping a fixed time so a slow machine does not make it flaky.
    pub async fn wait_live(&self, id: &str) {
        for _ in 0..200 {
            let status = self.app.mixer.status().await.expect("status");
            if status
                .sources
                .iter()
                .any(|s| s.id == id && s.state == godwinmix_protocol::types::SourceState::Live)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("{id} never went live");
    }

    pub async fn call(&self, token: &Token, method: &str, params: Value) -> Result<Value, RpcError> {
        call::dispatch(
            &self.registry,
            &self.app,
            &self.snapshots,
            token,
            "test-trace",
            method,
            params,
        )
        .await
    }

    pub async fn program(&self) -> Option<String> {
        self.app.mixer.status().await.expect("status").program
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.app.mixer.send(mixer::Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn desk() -> Token {
    Token { id: "desk".into(), ..Token::open() }
}
