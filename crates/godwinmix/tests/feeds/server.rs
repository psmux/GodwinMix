//! A feed server on loopback, changed between fetches by the test.
//!
//! `/feed` serves whatever the test last put there, with an ETag of its
//! own, and answers `304` to a request that sends that ETag back. `/hang`
//! never answers. `/ws` sends what the test pushes, one message each.
//! `/events` is the same as an event stream. Nothing here reaches the
//! internet.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use parking_lot::Mutex;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Default)]
pub struct Served {
    pub body: String,
    pub content_type: String,
    pub version: u32,
    pub requests: u32,
    pub not_modified: u32,
    /// The `x-api-key` header the last request carried.
    pub key: Option<String>,
}

#[derive(Clone)]
pub struct Server {
    pub base: String,
    pub served: Arc<Mutex<Served>>,
    push: broadcast::Sender<String>,
}

impl Server {
    pub async fn start() -> Server {
        let served = Arc::new(Mutex::new(Served::default()));
        let (push, _) = broadcast::channel(16);
        let server = Server { base: String::new(), served, push };
        let app = Router::new()
            .route("/feed", get(feed))
            .route("/hang", get(hang))
            .route("/ws", get(ws))
            .route("/events", get(events))
            .with_state(server.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        tokio::spawn(async move { axum::serve(listener, app).await.ok() });
        Server { base, ..server }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.base)
    }

    /// Serve `body` from now on, as a new version.
    pub fn put(&self, content_type: &str, body: &str) {
        let mut s = self.served.lock();
        s.body = body.to_string();
        s.content_type = content_type.to_string();
        s.version += 1;
    }

    pub fn push(&self, message: &str) {
        let _ = self.push.send(message.to_string());
    }

    pub fn receivers(&self) -> usize {
        self.push.receiver_count()
    }
}

async fn feed(State(server): State<Server>, headers: HeaderMap) -> Response {
    let mut s = server.served.lock();
    s.requests += 1;
    s.key = headers.get("x-api-key").and_then(|v| v.to_str().ok()).map(str::to_string);
    let etag = format!("\"v{}\"", s.version);
    if headers.get("if-none-match").and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        s.not_modified += 1;
        return (StatusCode::NOT_MODIFIED, [("etag", etag)]).into_response();
    }
    (StatusCode::OK, [("etag", etag), ("content-type", s.content_type.clone())], s.body.clone()).into_response()
}

async fn hang() -> Response {
    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    StatusCode::OK.into_response()
}

async fn ws(State(server): State<Server>, upgrade: WebSocketUpgrade) -> Response {
    let rx = server.push.subscribe();
    upgrade.on_upgrade(move |socket| relay(socket, rx))
}

async fn relay(mut socket: WebSocket, mut rx: broadcast::Receiver<String>) {
    while let Ok(text) = rx.recv().await {
        if socket.send(Message::Text(text.into())).await.is_err() {
            return;
        }
    }
}

async fn events(State(server): State<Server>) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let rx = server.push.subscribe();
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        let text = rx.recv().await.ok()?;
        Some((Ok(Event::default().data(text)), rx))
    });
    Sse::new(stream)
}
