//! `config.get`, `config.set`, `config.reset` and `config.schema` on a real core.
//!
//! One running mixer, started from a commented config file on disk, with the
//! whole method table in front of it. What is checked is what a settings
//! dialog relies on: a live key really is live, a restart key says so, a bad
//! value is refused with what would have worked and writes nothing, and the
//! file keeps every comment it had.

use godwinmix::control::{call, methods, AppState};
use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::{self, Mixer};
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::Token;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;


const FILE: &str = "\
# The hall mixer. Written by hand; keep this line.
[canvas]
width = 320 # small, for the test
height = 180
fps = 30

[multiview]
enabled = false

[control]
token = \"s3cret-hall\"
";

struct Core {
    app: AppState,
    snapshots: Arc<Tracker>,
    registry: Registry<call::Call>,
    thread: Option<std::thread::JoinHandle<()>>,
    path: PathBuf,
}

impl Core {
    async fn start(tag: &str) -> Core {
        let _ = gstreamer::init();
        let dir = std::env::temp_dir().join(format!("gmx-config-methods-{tag}-{}", std::process::id()));
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
        let multiview = mix.multiview_handle();
        let preview = mix.preview_handle();
        let encoder = mix.encoder_handle();
        let thread = mixer::spawn(mix, cmd_rx, handle.clone());
        let snapshots = Tracker::new(cfg.snapshot.clone(), multiview.clone(), handle.clone());
        let library = Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone()));
        let converter = Arc::new(godwinmix_core::convert::Converter::new(handle.clone(), 1, 1));
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
                scenes: godwinmix_core::scene::server::SceneServer::in_memory(
                    godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
                ),
                plugins: godwinmix_core::plugin::supervisor::Supervisor::new(
                    godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
                    Default::default(),
                ),
            },
            false,
        );
        methods::config::configure(&cfg, &[]);
        let core = Core { app, snapshots, registry: methods::registry(), thread: Some(thread), path };
        for id in ["cam1", "cam2"] {
            core.add_source(id).await;
        }
        core
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

    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let token = Token { id: "admin".into(), ..Token::open() };
        call::dispatch(&self.registry, &self.app, &self.snapshots, &token, "test", method, params).await
    }

    fn file(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap()
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

fn key<'a>(got: &'a Value, name: &str) -> &'a Value {
    got["keys"].as_array().unwrap().iter().find(|k| k["key"] == name).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_settings_round_trip_through_the_protocol() {
    let core = Core::start("round-trip").await;

    // Read: the file's value, the default, and the secret kept secret.
    let got = core.call("config.get", json!({})).await.unwrap();
    assert_eq!(key(&got, "canvas.width")["value"], 320);
    assert_eq!(key(&got, "canvas.width")["source"], "file");
    assert_eq!(key(&got, "program.video_bitrate_kbps")["source"], "default");
    let token = key(&got, "control.token");
    assert_eq!(token["value"], Value::Null);
    assert_eq!(token["set"], true);
    assert!(!got.to_string().contains("s3cret-hall"), "the token never leaves the core");
    assert_eq!(got["needs_restart"], json!([]));

    // A live key is live: the minimum hold refuses the second take at once.
    let set = core.call("config.set", json!({ "values": { "safety.min_hold_ms": 60000 } })).await.unwrap();
    assert_eq!(set["applied"], json!(["safety.min_hold_ms"]));
    assert_eq!(set["changed"][0]["applies"], "live");
    core.call("program.take", json!({ "source": "cam1" })).await.unwrap();
    let held = core.call("program.take", json!({ "source": "cam2" })).await.unwrap_err();
    assert_eq!(held.code, -32003, "the new hold refused the take: {}", held.message);

    // A restart key says so, and stays on the list until the file and the
    // running core agree again.
    let set = core.call("config.set", json!({ "values": { "canvas.width": 640 } })).await.unwrap();
    assert_eq!(set["changed"][0]["applies"], "restart");
    assert_eq!(set["needs_restart"], json!(["canvas.width"]));
    let back = core.call("config.set", json!({ "values": { "canvas.width": 320 } })).await.unwrap();
    assert_eq!(back["needs_restart"], json!([]));

    // Reset takes the key out, so the default applies from the next start.
    let reset = core.call("config.reset", json!({ "keys": ["canvas.width"] })).await.unwrap();
    assert_eq!(reset["needs_restart"], json!(["canvas.width"]));
    assert!(!core.file().contains("width = "), "{}", core.file());

    // A bad value is refused with the range, and nothing is written.
    let before = core.file();
    let e = core.call("config.set", json!({ "values": { "program.video_bitrate_kbps": 5, "safety.flash_guard": false } })).await.unwrap_err();
    assert_eq!(e.code, -32602);
    assert_eq!(e.data["minimum"], 100);
    assert_eq!(e.data["key"], "program.video_bitrate_kbps");
    assert_eq!(core.file(), before, "all or nothing");

    // The sentinel leaves the token alone; the file keeps its comments.
    let kept = core.call("config.set", json!({ "values": { "control.token": "__secret__", "stall.hold_last_frame": false } })).await.unwrap();
    assert_eq!(kept["unchanged"], json!(["control.token"]));
    let text = core.file();
    assert!(text.starts_with("# The hall mixer. Written by hand; keep this line.\n"), "{text}");
    assert!(text.contains("s3cret-hall"), "{text}");
    assert!(text.contains("[stall]\nhold_last_frame = false"), "{text}");

    // What config.set does not own names what does.
    let e = core.call("config.set", json!({ "values": { "sources.cam1": {} } })).await.unwrap_err();
    assert!(e.message.contains("source.add"), "{}", e.message);

    let schema = core.call("config.schema", json!({})).await.unwrap();
    assert_eq!(schema["properties"]["safety.min_hold_ms"]["x-gmx-applies"], "live");
}

/// A refusal that names a setting carries it as `data.action`, and doing
/// what the action says is enough: the exec switch is live, so the same add
/// goes through straight after.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusals_action_is_the_setting_that_lets_it_through() {
    let core = Core::start("actions").await;

    let add = json!({ "id": "cmd", "uri": "exec:sleep 30" });
    let e = core.call("source.add", add.clone()).await.unwrap_err();
    let action = &e.data["action"];
    assert_eq!(action["kind"], "set-config", "{e}");
    assert_eq!(action["key"], "security.allow_exec_sources");
    assert_eq!(action["applies"], "live");
    let set = core
        .call("config.set", json!({ "values": { action["key"].as_str().unwrap(): action["value"].clone() } }))
        .await
        .unwrap();
    assert_eq!(set["applied"], json!(["security.allow_exec_sources"]));
    core.call("source.add", add).await.expect("the same add, allowed now");

    // The multiview is off in this file, so a snapshot says which switch and
    // offers it, with the restart it needs.
    let e = core.call("snapshot.get", json!({ "id": "sheet" })).await.unwrap_err();
    assert_eq!(e.data["action"]["key"], "multiview.enabled", "{e}");
    assert_eq!(e.data["action"]["applies"], "restart");
    assert!(e.message.contains("restart the mixer"), "{}", e.message);

    // The take guard names the hold in seconds and offers the live setting.
    core.call("config.set", json!({ "values": { "safety.min_hold_ms": 60000 } })).await.unwrap();
    core.call("program.take", json!({ "source": "cam1" })).await.unwrap();
    let held = core.call("program.take", json!({ "source": "cam2" })).await.unwrap_err();
    assert_eq!(held.data["action"]["key"], "safety.min_hold_ms", "{held}");
    assert!(held.message.contains(" s,") || held.message.contains(" s "), "{}", held.message);
    assert!(!held.message.contains("[safety]"), "{}", held.message);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_doctor_reads_the_config_the_core_was_started_with() {
    let core = Core::start("doctor").await;
    let got = core.call("core.doctor", json!({})).await.unwrap();
    let config = got["checks"].as_array().unwrap().iter().find(|c| c["name"] == "config").unwrap();
    let detail = config["detail"].as_str().unwrap();
    assert!(detail.contains(&core.path.display().to_string()), "{detail}");
}
