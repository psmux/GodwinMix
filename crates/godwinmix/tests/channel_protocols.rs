//! Channels on every protocol, on a real core: the protocols a channel has
//! on, RTMPS and its port, the ports the channels need and why, and the
//! certificate RTMPS answers with.

use godwinmix::control::{call, methods, AppState};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{Mixer, MixerHandle};
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::Token;
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


#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_channel_takes_the_protocols_switched_on_and_says_which_ports_it_needs() {
    let dir = std::env::temp_dir().join(format!("gmx-channel-protocols-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("GODWINMIX_HOME", dir.join("home"));
    let path = dir.join("godwinmix.toml");
    std::fs::write(&path, format!("{FILE}\n[plugins.ingest]\nrtmp_port = 19381\nsrt_port = 19382\n")).unwrap();
    let core = start(&path);

    let added = core.call("channel.add", json!({"name": "Church", "protocols": ["rtmp", "srt"]})).await.unwrap();
    let channel = &added["channel"];
    assert_eq!(channel["protocols"], json!(["rtmp", "srt"]));
    let addresses = channel["publish"]["addresses"].as_array().unwrap();
    let srt = addresses.iter().find(|a| a["protocol"] == "srt").expect("an SRT address");
    assert!(srt["server"].as_str().unwrap().ends_with(":19382"), "{srt}");
    assert!(srt["example"].as_str().unwrap().contains("streamid=church/main&passphrase=<key>"), "{srt}");

    let none = core.call("channel.set", json!({"id": "church", "protocols": []})).await.unwrap_err();
    assert!(none.message.contains("at least one way in"), "{}", none.message);
    assert_eq!(none.data["field"], "protocols");

    let clash = core.call("channel.set", json!({"id": "church", "rtmps": {"enabled": true, "port": 19381}})).await.unwrap_err();
    assert!(clash.message.contains("RTMP port"), "{}", clash.message);
    let secure = core
        .call("channel.set", json!({"id": "church", "protocols": ["whip"], "rtmps": {"enabled": true, "port": 8443}}))
        .await
        .unwrap();
    assert_eq!(secure["rtmps"], json!({"enabled": true, "port": 8443}));
    let kinds: Vec<&str> = secure["publish"]["addresses"].as_array().unwrap().iter().map(|a| a["protocol"].as_str().unwrap()).collect();
    assert_eq!(kinds, vec!["rtmps", "whip"]);

    // No ingest plugin runs in this test, so every wanted port says why not.
    let list = core.call("channel.list", json!({})).await.unwrap();
    let rows = list["listeners"].as_array().unwrap();
    let whip = rows.iter().find(|r| r["protocol"] == "whip").expect("a WHIP row");
    assert_eq!(whip["because"], json!(["church"]));
    assert_eq!(whip["open"], false);
    assert!(whip["problem"].as_str().unwrap_or("").contains("ingest"), "{whip}");
    let rtmp = rows.iter().find(|r| r["protocol"] == "rtmp").unwrap();
    assert_eq!(rtmp["because"], json!([]), "RTMP was switched off, so nothing wants its port");

    let made = core.call("channel.certificate.generate", json!({"names": ["mixer.local"]})).await.unwrap();
    assert_eq!(made["source"], "self_signed");
    assert_eq!(made["names"], json!(["mixer.local"]));
    let bad = core.call("channel.certificate.set", json!({"cert": "nonsense", "key": "nonsense"})).await.unwrap_err();
    assert!(bad.message.contains("BEGIN CERTIFICATE"), "{}", bad.message);
    let list = core.call("channel.list", json!({})).await.unwrap();
    assert_eq!(list["certificate"]["fingerprint"], made["fingerprint"]);
    assert!(!list.to_string().contains("PRIVATE KEY"), "the key is never in a list");

    let file = std::fs::read_to_string(dir.join("godwinmix.runtime.channels.toml")).unwrap();
    assert!(file.contains("protocols = [\"whip\"]"), "{file}");
    assert!(!file.contains("BEGIN"), "no certificate or key in the channels file: {file}");
    std::fs::remove_dir_all(&dir).ok();
}
