//! Two people on one token, over two real `/rpc` connections.
//!
//! A phone and a laptop on the same token used to be one client: both were
//! "default", each read the other's edits as its own echo, and Ctrl+Z on one
//! took back what the other had just done. This opens two WebSockets on one
//! core with one token and checks the three things that fixes: each has a
//! client id of its own, each is told when the other comes and goes, and each
//! undoes only its own change.
//!
//! A real server on a real port with a real mixer, as `patches.rs` does,
//! because the ids and the presence stream live in the connection loop.

use futures_util::{SinkExt, StreamExt};
use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// A core with one scene of two named boxes, listening on a port the
/// operating system picked. Loopback unless `GMX_TEST_BIND` names an address,
/// for a machine whose VPN resets loopback connections.
async fn serve() -> String {
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
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));

    let scenes = godwinmix_core::scene::server::SceneServer::in_memory(
        godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
    );
    scenes
        .edit(None, |doc| {
            let mut scene = godwinmix_core::scene::Scene::new("wide");
            for name in ["left", "right"] {
                let mut item = godwinmix_core::scene::Item::new(
                    godwinmix_core::scene::Content::Source { source: format!("cam-{name}") },
                );
                item.name = Some(name.into());
                scene.items.push(item);
            }
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
            quit: Arc::new(tokio::sync::Notify::new()),
            scenes,
            plugins: godwinmix_core::plugin::supervisor::Supervisor::detached(),
        },
        false,
    );
    let host = std::env::var("GMX_TEST_BIND").unwrap_or_else(|_| "127.0.0.1".into());
    let listener = tokio::net::TcpListener::bind(format!("{host}:0")).await.expect("a port");
    let address = listener.local_addr().expect("the port it picked");
    tokio::spawn(async move {
        let _ = godwinmix::control::serve_on(listener, app).await;
    });
    format!("ws://{address}/rpc")
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// One person: connected, subscribed to scenes and presence.
struct Person {
    socket: Socket,
    next_id: u64,
    client_id: String,
}

impl Person {
    async fn open(url: &str) -> Person {
        let (socket, _) = tokio_tungstenite::connect_async(url).await.expect("connecting to /rpc");
        let mut person = Person { socket, next_id: 1, client_id: String::new() };
        let answer = person
            .call("core.subscribe", json!({ "events": ["scene.*", "presence.*", "flush"] }))
            .await
            .expect("subscribing");
        person.client_id = answer["client_id"].as_str().expect("a client id").to_string();
        person
    }

    /// The answer, or the error object when the core refused.
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.socket.send(Message::Text(request.to_string().into())).await.expect("sending");
        for _ in 0..80 {
            let Some(frame) = self.read().await else { break };
            if frame.get("id").and_then(Value::as_u64) == Some(id) {
                return match frame.get("result") {
                    Some(result) => Ok(result.clone()),
                    None => Err(frame["error"].clone()),
                };
            }
        }
        Err(json!({ "message": format!("{method} was never answered") }))
    }

    async fn read(&mut self) -> Option<Value> {
        loop {
            match tokio::time::timeout(Duration::from_secs(5), self.socket.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => return serde_json::from_str(&text).ok(),
                Ok(Some(Ok(_))) => continue,
                _ => return None,
            }
        }
    }

    /// The next presence list whose client ids satisfy `wanted`.
    async fn presence_until(&mut self, wanted: impl Fn(&[String]) -> bool) -> Vec<String> {
        for _ in 0..80 {
            let Some(frame) = self.read().await else { break };
            if frame["method"] != "event/presence.changed" {
                continue;
            }
            let ids: Vec<String> = frame["params"]["clients"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c["client_id"].as_str().map(str::to_string))
                .collect();
            if wanted(&ids) {
                return ids;
            }
        }
        panic!("the presence list never got there");
    }

    async fn move_to(&mut self, item: &str, x: f64) {
        self.call(
            "scene.item.set",
            json!({"scene": "wide", "item": item, "props": {"transform": {"position": {"x": x, "y": 0.0}}}}),
        )
        .await
        .expect("a move");
    }
}

/// Where an item is, read back from the core.
async fn x_of(person: &mut Person, item: &str) -> f64 {
    let view = person.call("scene.get", json!({"scene": "wide"})).await.expect("the scene");
    let id = view["records"]
        .as_array()
        .and_then(|r| r.iter().find(|r| r["name"] == item))
        .map(|r| r["id"].clone())
        .expect("the item");
    let boxes = view["geometry"].as_array().expect("geometry");
    boxes.iter().find(|g| g["item"] == id).and_then(|g| g["x"].as_f64()).expect("its box")
}

#[tokio::test(flavor = "multi_thread")]
async fn two_people_on_one_token_are_two_clients_with_their_own_undo() {
    let url = serve().await;
    let mut phone = Person::open(&format!("{url}?client_id=phone")).await;
    let mut laptop = Person::open(&url).await;

    // Two client ids, one token.
    assert_eq!(phone.client_id, "open.phone", "the name the page chose was not kept");
    assert_ne!(phone.client_id, laptop.client_id, "two connections share a client id");
    let token_of = |id: &str| id.split('.').next().unwrap_or("").to_string();
    assert_eq!(token_of(&phone.client_id), token_of(&laptop.client_id));
    let info = laptop.call("core.info", json!({})).await.expect("core.info");
    assert_eq!(info["client_id"], laptop.client_id.as_str(), "core.info names another client");

    // The phone is told the laptop arrived.
    let laptop_id = laptop.client_id.clone();
    phone.presence_until(|ids| ids.contains(&laptop_id)).await;

    // The laptop says what it is editing, and the phone sees it.
    let listed = laptop.call("presence.set", json!({"scene": "wide"})).await.expect("presence.set");
    let me = listed["clients"].as_array().unwrap().iter().find(|c| c["you"] == true).cloned();
    assert_eq!(me.expect("the caller is marked")["client_id"], laptop.client_id.as_str());

    // Each moves a different box, and each patch says whose it was.
    phone.move_to("left", 100.0).await;
    laptop.move_to("right", 200.0).await;
    let undone = phone.call("scene.undo", json!({})).await.expect("the phone's undo");
    assert_eq!(undone["patch"]["source_client"], phone.client_id.as_str());
    assert_eq!(x_of(&mut phone, "left").await, 0.0, "the phone's move was not undone");
    assert_eq!(x_of(&mut phone, "right").await, 200.0, "the phone undid the laptop's move");

    // Undoing over somebody else's later change is refused, and names them.
    laptop.move_to("left", 300.0).await;
    phone.call("scene.redo", json!({})).await.expect_err("the laptop moved it since");
    phone.move_to("right", 50.0).await;
    laptop.move_to("right", 250.0).await;
    let refused = phone.call("scene.undo", json!({})).await.expect_err("refused");
    assert_eq!(refused["data"]["conflict"], "undo", "{refused}");
    assert_eq!(refused["data"]["conflicts"][0]["changed_by"], laptop.client_id.as_str(), "{refused}");
    phone.call("scene.undo", json!({"force": true})).await.expect("forced");
    assert_eq!(x_of(&mut laptop, "right").await, 0.0);

    // The laptop goes, and the phone is told.
    drop(laptop);
    let phone_id = phone.client_id.clone();
    phone.presence_until(|ids| ids == [phone_id.clone()]).await;
}
