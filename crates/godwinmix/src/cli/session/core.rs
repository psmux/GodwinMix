//! The core a session is replayed against.
//!
//! 1280x720x30, no outputs, no multiview: the test core of 03 section 11 and
//! `godwinmix --test-core`, built in this process so a replay needs no daemon,
//! no port and no config file. Sources are the deterministic `test/source`
//! double, which is what lets a session recorded in a church hall run on a
//! laptop with no network (10 section 4).

use anyhow::{Context, Result};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::Token;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::control::{call, methods, AppState, Engine};

/// How long to wait after the last command for the events it caused. A take
/// lands on the next frame boundary and a source goes live when it produces
/// its first frame; both are well inside this.
const SETTLE: Duration = Duration::from_millis(1_200);

pub struct TestCore {
    app: AppState,
    snapshots: Arc<Tracker>,
    registry: Registry<call::Call>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TestCore {
    pub async fn start() -> Result<TestCore> {
        gstreamer::init().context("initialising GStreamer for the replay")?;
        let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
        cfg.canvas.width = 1280;
        cfg.canvas.height = 720;
        cfg.canvas.fps = 30;
        cfg.multiview.enabled = false;
        // A replay re-issues what was recorded. Holding it to the minimum hold
        // would refuse takes the original run made, and the answer would be a
        // difference in the log rather than in the code under test.
        cfg.safety.min_hold_ms = 0;
        cfg.safety.flash_guard = false;

        let (mut mix, handle, cmd_rx, mut bus_rx) =
            Mixer::build(cfg.clone()).context("building the test core")?;
        mix.start().context("starting the test core")?;
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
        let snapshots = Tracker::new(cfg.snapshot.clone(), multiview.clone(), handle.clone());
        let library = Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone()));
        let converter = Arc::new(godwinmix_core::convert::Converter::new(
            handle.clone(),
            library.cfg().convert_threads,
            library.cfg().probe_timeout_secs,
        ));
        // A replay's scenes live in memory only: a test core never writes a
        // collection beside anyone's runtime store.
        let scenes = godwinmix_core::scene::server::SceneServer::in_memory(
            godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
        );
        let app = AppState::new(
            &cfg,
            Engine {
                mixer: handle,
                multiview,
                preview,
                encoder,
                library,
                converter,
                scenes,
                quit: Arc::new(tokio::sync::Notify::new()),
                // A test core runs no plugin singletons; the supervisor is
                // here because the control plane asks it what transitions exist.
                plugins: godwinmix_core::plugin::supervisor::Supervisor::new(
                    godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
                    Default::default(),
                ),
            },
            false,
        );
        let core =
            TestCore { app, snapshots, registry: methods::registry(), thread: Some(thread) };
        crate::control::spawn_background(core.app.clone());
        Ok(core)
    }

    /// Everything published from here on. Taken before the first command so
    /// nothing a replay causes is missed.
    pub fn watch(&self) -> tokio::sync::broadcast::Receiver<godwinmix_core::state::Envelope> {
        self.app.mixer.subscribe()
    }

    /// Issue one recorded command through the same dispatcher `/rpc` uses.
    ///
    /// One protocol: a replay that called into the engine directly would not
    /// be exercising what a client exercises, and the whole point of the
    /// session log is that it records what clients asked for.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        call::dispatch(
            &self.registry,
            &self.app,
            &self.snapshots,
            &replayer(),
            "replay",
            method,
            params,
        )
        .await
    }

    pub async fn settle(&self) {
        tokio::time::sleep(SETTLE).await;
    }

    /// What is on air now, for a report.
    pub async fn program(&self) -> Option<String> {
        self.app.mixer.status().await.ok().and_then(|s| s.program)
    }
}

impl Drop for TestCore {
    fn drop(&mut self) {
        let _ = self.app.mixer.send(mixer::Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The token a replay issues commands under.
///
/// Its own id, not the recorded one, so `program.history` and the session log
/// of the replay both say plainly that a machine did this and not the person
/// whose name is in the original.
fn replayer() -> Token {
    Token { id: "replay".into(), ..Token::open() }
}
