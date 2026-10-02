//! Asking for a source whose first party plugin is not installed installs the
//! plugin and brings the source up, with one call and no Install step.
//!
//! A real server on a real port, real GStreamer, a scratch plugins folder and
//! the `udp` plugin from this checkout. The first run builds the plugin's
//! release binary through its `[build]` section, so it can take a few minutes
//! on a cold target.

use futures_util::{SinkExt, StreamExt};
use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

async fn serve(home: &std::path::Path) -> String {
    let _ = gstreamer::init();
    godwinmix_core::plugin::loader::set_dir(home.join("plugins"));
    godwinmix_core::plugin::loader::load_all(&Default::default());
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    cfg.canvas.width = 320;
    cfg.canvas.height = 180;
    cfg.canvas.fps = 15;
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building the mixer");
    mix.start().expect("starting the mixer");
    let (multiview, preview, encoder) = (mix.multiview_handle(), mix.preview_handle(), mix.encoder_handle());
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));
    let scenes = godwinmix_core::scene::server::SceneServer::in_memory(godwinmix_core::caps::CanvasCaps::new(&cfg.canvas));
    let app = AppState::new(
        &cfg,
        Engine {
            mixer: handle.clone(),
            multiview,
            preview,
            encoder,
            library: Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone())),
            converter: Arc::new(godwinmix_core::convert::Converter::new(handle.clone(), 1, 5)),
            quit: Arc::new(tokio::sync::Notify::new()),
            scenes,
            plugins: godwinmix_core::plugin::supervisor::Supervisor::detached(),
        },
        false,
    );
    godwinmix::setup::attach(app.clone(), cfg.browser.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("the port it picked");
    tokio::spawn(async move {
        let _ = godwinmix::control::serve_on(listener, app).await;
    });
    format!("ws://{address}/rpc")
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// One call's whole answer, skipping the events in between.
async fn ask(socket: &mut Socket, id: u64, method: &str, params: Value) -> Value {
    let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    socket.send(Message::Text(request.to_string().into())).await.expect("sending");
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(30), socket.next())
            .await
            .expect("an answer in time")
            .expect("an open socket")
            .expect("a frame");
        let Message::Text(text) = frame else { continue };
        let value: Value = serde_json::from_str(&text).expect("JSON");
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            return value;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_source_whose_first_party_plugin_is_missing_installs_it_and_comes_up() {
    let home = std::env::temp_dir().join(format!("gmx-setup-first-use-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    // Before anything reads it: the install and the setup log land here.
    std::env::set_var("GODWINMIX_HOME", &home);
    let url = serve(&home).await;
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.expect("connecting to /rpc");

    let before = ask(&mut socket, 1, "setup.get", json!({ "piece": "udp" })).await;
    assert_eq!(before["result"]["state"], "missing", "{before}");
    let message = before["result"]["message"].as_str().unwrap_or_default();
    assert!(!message.contains("udp") && !message.contains("plugin"), "a plain sentence: {message}");

    // One call. The source is answered as waiting, with the set up under it.
    let added = ask(
        &mut socket,
        2,
        "source.add",
        json!({ "id": "feed", "uri": "udp://127.0.0.1:47123", "type": "udp/source" }),
    )
    .await;
    let record = added.get("result").unwrap_or_else(|| panic!("source.add was refused: {added}"));
    assert_eq!(record["id"], "feed");
    assert!(record["setup"]["piece"] == "udp", "the set up rides on the answer: {record}");

    // The install runs in the background; the source starts by itself.
    let mut next = 3;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
    loop {
        let list = ask(&mut socket, next, "source.list", json!({})).await;
        next += 1;
        let here = list["result"].as_array().into_iter().flatten().any(|s| s["id"] == "feed")
            || list["result"]["sources"].as_array().into_iter().flatten().any(|s| s["id"] == "feed");
        if here {
            break;
        }
        let setup = ask(&mut socket, next, "setup.get", json!({ "piece": "udp" })).await;
        next += 1;
        assert_ne!(setup["result"]["state"], "failed", "the install did not finish: {setup}");
        assert!(tokio::time::Instant::now() < deadline, "the source never came up: {setup}");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let after = ask(&mut socket, next, "setup.get", json!({ "piece": "udp" })).await;
    assert_eq!(after["result"]["state"], "ready", "{after}");
    assert!(home.join("plugins").join("udp").is_dir(), "installed into the scratch home");
    let _ = std::fs::remove_dir_all(&home);
}
