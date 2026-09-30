//! A scene that draws a source this mixer does not have goes to air anyway.
//!
//! The take used to be refused, which kept a whole service off air over one
//! camera that was not plugged in. Now the take goes ahead with what is here,
//! and the answer, `program.get`, the arm and an alert name what is missing.
//! A scene with nothing here at all is still refused, because taking it would
//! only swap the picture for the slate.
//!
//! A real server on a real port, real GStreamer, two test patterns.

use futures_util::{SinkExt, StreamExt};
use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

async fn serve() -> String {
    let _ = gstreamer::init();
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    cfg.canvas.width = 320;
    cfg.canvas.height = 180;
    cfg.canvas.fps = 15;
    cfg.sources = [("bars", "smpte"), ("ball", "ball")]
        .iter()
        .map(|(id, pattern)| {
            toml::from_str(&format!("id = \"{id}\"\ntype = \"test/source\"\nuri = \"test://{pattern}\"\n"))
                .expect("a valid source document")
        })
        .collect();
    cfg.safety.min_hold_ms = 0;

    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building the mixer");
    mix.start().expect("starting the mixer");
    let multiview = mix.multiview_handle();
    let preview = mix.preview_handle();
    let encoder = mix.encoder_handle();
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));
    let scenes = godwinmix_core::scene::server::SceneServer::in_memory(
        godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
    );
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
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("the port it picked");
    tokio::spawn(async move {
        let _ = godwinmix::control::serve_on(listener, app).await;
    });
    format!("ws://{address}/rpc")
}

struct Client {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: u64,
    /// Every alert message this client has been sent.
    alerts: Vec<String>,
}

impl Client {
    async fn open(url: &str) -> Client {
        let (socket, _) = tokio_tungstenite::connect_async(url).await.expect("connecting to /rpc");
        Client { socket, next_id: 1, alerts: Vec::new() }
    }

    /// The whole answer: `result` or `error`.
    async fn ask(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.socket.send(Message::Text(request.to_string().into())).await.expect("sending");
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(10), self.socket.next())
                .await
                .expect("an answer in time")
                .expect("an open socket")
                .expect("a frame");
            let Message::Text(text) = frame else { continue };
            let value: Value = serde_json::from_str(&text).expect("JSON");
            self.remember(&value);
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value;
            }
        }
    }

    async fn call(&mut self, method: &str, params: Value) -> Value {
        let answer = self.ask(method, params).await;
        answer.get("result").cloned().unwrap_or_else(|| panic!("{method} failed: {answer}"))
    }

    fn remember(&mut self, value: &Value) {
        if value.get("method").and_then(Value::as_str) == Some("event/alert") {
            let message = value["params"]["message"].as_str().unwrap_or_default();
            self.alerts.push(message.to_string());
        }
    }

    /// Read events until an alert mentioning `word` arrives, or time is up.
    async fn alert_about(&mut self, word: &str) -> Option<String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(found) = self.alerts.iter().find(|m| m.contains(word)) {
                return Some(found.clone());
            }
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            let Ok(Some(Ok(Message::Text(text)))) =
                tokio::time::timeout(left, self.socket.next()).await
            else {
                return None;
            };
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                self.remember(&value);
            }
        }
    }
}

async fn scene(client: &mut Client, sources: &[&str], name: &str) -> String {
    let made = client.call("scene.create_from", json!({ "sources": sources, "name": name })).await;
    made["id"].as_str().expect("the new scene's id").to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scene_missing_a_source_goes_to_air_and_names_it() {
    let url = serve().await;
    let mut client = Client::open(&url).await;
    client.call("core.subscribe", json!({ "events": ["alert", "program.*"] })).await;
    let two = scene(&mut client, &["bars", "ball"], "two box").await;
    client.call("source.remove", json!({ "id": "ball" })).await;

    // Armed in Preview, the answer already says what the take will lack.
    let armed = client.call("scene.preview.set", json!({ "scene": two })).await;
    assert_eq!(armed["missing"], json!(["ball"]), "{armed}");

    // The take of the armed scene goes ahead.
    let taken = client.call("program.take", json!({})).await;
    assert_eq!(taken["scene"], json!("two box"), "{taken}");
    assert_eq!(taken["missing"], json!(["ball"]), "{taken}");
    let alert = client.alert_about("ball").await.expect("an alert naming the missing source");
    assert!(alert.contains("two box"), "{alert}");

    // And it stays named for as long as that scene is on air.
    let now = client.call("program.get", json!({})).await;
    assert_eq!(now["missing"], json!(["ball"]), "{now}");

    // A scene with everything here says nothing about it.
    let bars = scene(&mut client, &["bars"], "bars only").await;
    let clean = client.call("program.take", json!({ "scene": bars })).await;
    assert!(clean.get("missing").is_none(), "{clean}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scene_with_nothing_running_is_refused_and_the_programme_stays() {
    let url = serve().await;
    let mut client = Client::open(&url).await;
    let ball = scene(&mut client, &["ball"], "ball shot").await;
    client.call("program.take", json!({ "source": "bars" })).await;
    client.call("source.remove", json!({ "id": "ball" })).await;

    let refused = client.ask("program.take", json!({ "scene": ball })).await;
    let error = refused.get("error").unwrap_or_else(|| panic!("taken: {refused}"));
    assert_eq!(error["data"]["missing"], json!(["ball"]), "{error}");
    assert!(error["message"].as_str().unwrap_or_default().contains("bars"), "{error}");
    let now = client.call("program.get", json!({})).await;
    assert_eq!(now["program"], json!("bars"), "the refusal touched the programme: {now}");
}
