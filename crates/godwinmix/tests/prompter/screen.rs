//! One prompter screen on `/rpc`, as the page at `/prompter/` is.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub struct Screen {
    socket: Socket,
    next_id: u64,
}

impl Screen {
    /// Connected and subscribed to `events`, as the page subscribes to
    /// `prompter.*` and nothing else.
    pub async fn open(url: &str, events: &[&str]) -> Screen {
        let (socket, _) = tokio_tungstenite::connect_async(url).await.expect("connecting to /rpc");
        let mut screen = Screen { socket, next_id: 1 };
        screen.call("core.subscribe", json!({ "events": events })).await.expect("subscribing");
        screen
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.socket.send(Message::Text(request.to_string().into())).await.expect("sending");
        for _ in 0..80 {
            let Some(frame) = self.read(Duration::from_secs(5)).await else { break };
            if frame.get("id").and_then(Value::as_u64) == Some(id) {
                return match frame.get("result") {
                    Some(result) => Ok(result.clone()),
                    None => Err(frame["error"].clone()),
                };
            }
        }
        Err(json!({ "message": format!("{method} was never answered") }))
    }

    async fn read(&mut self, wait: Duration) -> Option<Value> {
        loop {
            match tokio::time::timeout(wait, self.socket.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => return serde_json::from_str(&text).ok(),
                Ok(Some(Ok(_))) => continue,
                _ => return None,
            }
        }
    }

    /// The next `event/<name>` within `wait`, or None.
    pub async fn event(&mut self, name: &str, wait: Duration) -> Option<Value> {
        let until = std::time::Instant::now() + wait;
        let method = format!("event/{name}");
        while let Some(left) = until.checked_duration_since(std::time::Instant::now()) {
            let frame = self.read(left).await?;
            if frame["method"] == method.as_str() {
                return Some(frame["params"].clone());
            }
        }
        None
    }
}
