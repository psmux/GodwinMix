//! `event/scene.patch` over two real `/rpc` connections.
//!
//! The designer is a client of the scene server and the core is authoritative
//! over the document (11 section 4), which only works if a change one client
//! makes reaches the others. This opens two WebSockets, edits from one, and
//! checks that both are told: the editor by a patch carrying its own
//! `source_client` and its own `client_seq`, so it can suppress the echo of
//! what it has already drawn, and the other by the same patch, so it can
//! redraw.
//!
//! A real server on a real port, because the pump lives in the connection loop
//! and calling the method directly would prove nothing about it.

use futures_util::{SinkExt, StreamExt};
use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// A core listening on a port the operating system picked.
async fn serve() -> (String, Arc<tokio::sync::Notify>) {
    let _ = gstreamer::init();
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    cfg.canvas.width = 320;
    cfg.canvas.height = 180;
    cfg.canvas.fps = 30;
    cfg.multiview.enabled = false;

    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building the mixer");
    mix.start().expect("starting the mixer");
    let multiview = mix.multiview_handle();
    let preview = mix.preview_handle();
    let encoder = mix.encoder_handle();
    // The mixer thread outlives the test on purpose: the test ends when the
    // assertions do and tearing a pipeline down to prove it can is another
    // test's job.
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));

    let quit = Arc::new(tokio::sync::Notify::new());
    let scenes = godwinmix_core::scene::server::SceneServer::in_memory(
        godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
    );
    // One scene with one item, so there is something to move.
    scenes
        .edit(None, |doc| {
            let mut scene = godwinmix_core::scene::Scene::new("wide");
            let mut item = godwinmix_core::scene::Item::new(
                godwinmix_core::scene::Content::Source { source: "cam1".into() },
            );
            item.name = Some("stage".into());
            scene.items.push(item);
            doc.scenes.push(scene);
            Ok(())
        })
        .expect("a scene to edit");

    let app = AppState::new(
        &cfg,
        Engine {
            mixer: handle.clone(),
            multiview,
            preview,
            encoder,
            library: Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone())),
            converter: Arc::new(godwinmix_core::convert::Converter::new(handle.clone(), 1, 5)),
            quit: quit.clone(),
            scenes,
            plugins: godwinmix_core::plugin::supervisor::Supervisor::detached(),
        },
        false,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("the port it picked");
    tokio::spawn(async move {
        let _ = godwinmix::control::serve_on(listener, app).await;
    });
    (format!("ws://{address}/rpc"), quit)
}

/// One client: connected, subscribed to the scene stream, and ready to read.
struct Client {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: u64,
}

impl Client {
    async fn open(url: &str) -> Client {
        let (socket, _) = tokio_tungstenite::connect_async(url).await.expect("connecting to /rpc");
        let mut client = Client { socket, next_id: 1 };
        client
            .call("core.subscribe", json!({ "events": ["scene.*", "flush"] }))
            .await
            .expect("subscribing");
        client
    }

    async fn call(&mut self, method: &str, params: Value) -> Option<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.socket
            .send(Message::Text(request.to_string().into()))
            .await
            .expect("sending a request");
        // The answer may arrive behind notifications the subscription started.
        for _ in 0..40 {
            let frame = self.read().await?;
            if frame.get("id").and_then(Value::as_u64) == Some(id) {
                return frame.get("result").cloned();
            }
        }
        None
    }

    /// The next text frame, or `None` when nothing arrives in time.
    async fn read(&mut self) -> Option<Value> {
        loop {
            let message = tokio::time::timeout(Duration::from_secs(5), self.socket.next()).await;
            match message {
                Ok(Some(Ok(Message::Text(text)))) => {
                    return serde_json::from_str(&text).ok();
                }
                Ok(Some(Ok(_))) => continue,
                _ => return None,
            }
        }
    }

    /// The next `event/scene.patch`, skipping whatever else is on the wire.
    async fn next_patch(&mut self) -> Option<Value> {
        for _ in 0..40 {
            let frame = self.read().await?;
            if frame.get("method").and_then(Value::as_str) == Some("event/scene.patch") {
                return frame.get("params").cloned();
            }
        }
        None
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn one_client_edits_and_both_are_told() {
    let (url, _quit) = serve().await;
    let mut editor = Client::open(&url).await;
    let mut watcher = Client::open(&url).await;

    // A move, with the client's own sequence number on it. This is the shape a
    // drag takes: predicted locally, sent, and reconciled when the echo lands.
    let answer = editor
        .call(
            "scene.item.set",
            json!({
                "scene": "wide",
                "item": "stage",
                "props": {"transform": {"position": {"x": 120.0, "y": 40.0}}},
                "seq": 77
            }),
        )
        .await
        .expect("the move was answered");
    assert!(answer.get("records").is_some() || answer.get("id").is_some(), "{answer}");

    let theirs = watcher.next_patch().await.expect("the watcher was never told");
    assert_eq!(theirs["scope"], "document");
    assert_eq!(
        theirs["updated"].as_array().map(Vec::len),
        Some(1),
        "moving one item should be one record: {theirs}"
    );
    assert!(theirs["seq"].as_u64().unwrap_or(0) > 0, "a patch carries the document's sequence");

    // The editor is told as well, and can tell the patch is its own.
    let mine = editor.next_patch().await.expect("the editor was never told");
    assert_eq!(mine["seq"], theirs["seq"], "both clients see the same change");
    assert!(
        mine["source_client"].as_str().is_some(),
        "a patch must say who asked for it so a client can suppress its own echo: {mine}"
    );
    assert_eq!(
        mine["client_seq"], 77,
        "the client's own sequence number has to come back, or a drag cannot reconcile: {mine}"
    );
}

/// A client that did not ask for `scene.*` is not sent patches. Nothing runs,
/// and nothing is written, unless somebody asked.
#[tokio::test(flavor = "multi_thread")]
async fn a_client_that_did_not_ask_for_scenes_is_not_sent_patches() {
    let (url, _quit) = serve().await;
    let (socket, _) = tokio_tungstenite::connect_async(&url).await.expect("connecting");
    let mut quiet = Client { socket, next_id: 1 };
    quiet
        .call("core.subscribe", json!({ "events": ["program.*"] }))
        .await
        .expect("subscribing to programme events only");

    let mut editor = Client::open(&url).await;
    editor
        .call(
            "scene.item.set",
            json!({"scene": "wide", "item": "stage", "props": {"opacity": 0.5}}),
        )
        .await
        .expect("the change");
    editor.next_patch().await.expect("the editor is subscribed and should be told");

    // And the quiet one hears nothing about scenes.
    let heard = tokio::time::timeout(Duration::from_millis(750), quiet.next_patch()).await;
    assert!(
        matches!(heard, Err(_) | Ok(None)),
        "a client that asked for program.* was sent a scene patch"
    );
}
