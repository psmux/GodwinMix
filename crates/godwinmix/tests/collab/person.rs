//! One person on `/rpc`, as `collab.rs` drives them.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// One person: connected, subscribed to scenes and presence.
pub struct Person {
    socket: Socket,
    next_id: u64,
    pub client_id: String,
}

impl Person {
    pub async fn open(url: &str) -> Person {
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
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
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

    pub async fn read(&mut self) -> Option<Value> {
        loop {
            match tokio::time::timeout(Duration::from_secs(5), self.socket.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => return serde_json::from_str(&text).ok(),
                Ok(Some(Ok(_))) => continue,
                _ => return None,
            }
        }
    }

    /// The next presence list whose client ids satisfy `wanted`.
    pub async fn presence_until(&mut self, wanted: impl Fn(&[String]) -> bool) -> Vec<String> {
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

    pub async fn move_to(&mut self, item: &str, x: f64) {
        self.call(
            "scene.item.set",
            json!({"scene": "wide", "item": item, "props": {"transform": {"position": {"x": x, "y": 0.0}}}}),
        )
        .await
        .expect("a move");
    }
}

/// Where an item is, read back from the core.
pub async fn x_of(person: &mut Person, item: &str) -> f64 {
    let view = person.call("scene.get", json!({"scene": "wide"})).await.expect("the scene");
    let id = view["records"]
        .as_array()
        .and_then(|r| r.iter().find(|r| r["name"] == item))
        .map(|r| r["id"].clone())
        .expect("the item");
    let boxes = view["geometry"].as_array().expect("geometry");
    boxes.iter().find(|g| g["item"] == id).and_then(|g| g["x"].as_f64()).expect("its box")
}
