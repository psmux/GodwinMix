//! A fake core: a WebSocket server on a loopback port that speaks the
//! subscribe and event shapes from `protocol.md`.
//!
//! It exists so the UI's state store and key map can be tested without
//! GStreamer, without a mixer, and without a screen. It answers
//! `core.subscribe` the way the core does (a result, then `event/snapshot`,
//! then `event/flush`), records every request the client sent so a test can
//! assert on the exact JSON, and can push any event or drop the link on
//! command.

#![allow(dead_code)]

use godwinmix_tui::app::App;
use godwinmix_tui::client::{self, Command, Config, Incoming};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;

use futures_util::{SinkExt, StreamExt};

/// What the fake core should do next.
#[derive(Debug, Clone)]
pub enum Push {
    Text(Value),
    Binary(Vec<u8>),
    /// Close the socket under the client, to see it come back.
    Drop,
}

#[derive(Clone)]
pub struct Fake {
    pub addr: SocketAddr,
    /// Every JSON-RPC request the client sent, in order.
    pub requests: Arc<Mutex<Vec<Value>>>,
    /// How many times a client has connected. The reconnect test counts this.
    pub connections: Arc<AtomicUsize>,
    /// The `MixerStatus` handed out with `event/snapshot`.
    pub snapshot: Arc<Mutex<Value>>,
    /// Canned answers, by method. `Err` is sent as a JSON-RPC error object.
    pub answers: Arc<Mutex<HashMap<String, Result<Value, Value>>>>,
    push: broadcast::Sender<Push>,
}

impl Fake {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a loopback port");
        let addr = listener.local_addr().unwrap();
        let (push, _) = broadcast::channel(64);
        let fake = Self {
            addr,
            requests: Arc::new(Mutex::new(Vec::new())),
            connections: Arc::new(AtomicUsize::new(0)),
            snapshot: Arc::new(Mutex::new(example_status())),
            answers: Arc::new(Mutex::new(HashMap::new())),
            push,
        };
        let server = fake.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let connection = server.clone();
                tokio::spawn(async move { connection.serve(stream).await });
            }
        });
        fake
    }

    pub fn url(&self) -> String {
        format!("ws://{}/rpc", self.addr)
    }

    pub fn send(&self, push: Push) {
        let _ = self.push.send(push);
    }

    pub fn event(&self, name: &str, params: Value) {
        self.send(Push::Text(json!({"jsonrpc": "2.0", "method": name, "params": params})));
    }

    /// A batch: the deltas, then the flush that makes them visible.
    pub fn flush(&self, seq: u64) {
        self.event("event/flush", json!({ "seq": seq }));
    }

    pub fn answer(&self, method: &str, result: Value) {
        self.answers.lock().unwrap().insert(method.to_string(), Ok(result));
    }

    pub fn refuse(&self, method: &str, code: i64, message: &str) {
        self.answers.lock().unwrap().insert(
            method.to_string(),
            Err(json!({"code": code, "message": message, "data": {"retryable": true}})),
        );
    }

    /// Every request for one method.
    pub fn calls(&self, method: &str) -> Vec<Value> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.get("method").and_then(Value::as_str) == Some(method))
            .cloned()
            .collect()
    }

    async fn serve(self, stream: tokio::net::TcpStream) {
        let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else { return };
        self.connections.fetch_add(1, Ordering::SeqCst);
        let mut pushes = self.push.subscribe();
        loop {
            tokio::select! {
                incoming = socket.next() => match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let Ok(request) = serde_json::from_str::<Value>(&text) else { continue };
                        self.requests.lock().unwrap().push(request.clone());
                        for reply in self.reply_to(&request) {
                            if socket.send(Message::Text(reply.to_string().into())).await.is_err() {
                                return;
                            }
                        }
                    }
                    Some(Ok(_)) => {}
                    _ => return,
                },
                push = pushes.recv() => match push {
                    Ok(Push::Text(value)) => {
                        if socket.send(Message::Text(value.to_string().into())).await.is_err() {
                            return;
                        }
                    }
                    Ok(Push::Binary(bytes)) => {
                        if socket.send(Message::Binary(bytes.into())).await.is_err() {
                            return;
                        }
                    }
                    Ok(Push::Drop) => return,
                    Err(broadcast::error::RecvError::Closed) => return,
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                },
            }
        }
    }

    /// The core's own order: the result first, then the snapshot, then the
    /// flush that makes it paintable.
    fn reply_to(&self, request: &Value) -> Vec<Value> {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or_default();
        if method == "core.subscribe" {
            let events = request
                .get("params")
                .and_then(|p| p.get("events"))
                .cloned()
                .unwrap_or_else(|| json!(["*"]));
            return vec![
                json!({"jsonrpc": "2.0", "id": id,
                       "result": {"seq": 1, "events": events, "ignored_ext": []}}),
                json!({"jsonrpc": "2.0", "method": "event/snapshot",
                       "params": {"seq": 1, "state": self.snapshot.lock().unwrap().clone()}}),
                json!({"jsonrpc": "2.0", "method": "event/flush", "params": {"seq": 1}}),
            ];
        }
        match self.answers.lock().unwrap().get(method) {
            Some(Ok(result)) => vec![json!({"jsonrpc": "2.0", "id": id, "result": result})],
            Some(Err(error)) => vec![json!({"jsonrpc": "2.0", "id": id, "error": error})],
            None => vec![json!({"jsonrpc": "2.0", "id": id, "result": {}})],
        }
    }
}

/// Two sources and one destination: enough to take, mute and filter.
pub fn example_status() -> Value {
    json!({
        "sources": [
            {"id": "cam1", "name": "Stage camera", "uri": "rtmp://host/live/cam1",
             "state": "live", "has_video": true, "has_audio": true, "gain": 1.0,
             "muted": false, "seekable": false},
            {"id": "clip1", "name": "Opening titles", "uri": "file:///titles.mp4",
             "state": "live", "has_video": true, "has_audio": true, "gain": 1.0,
             "muted": false, "seekable": true, "position_ms": 0, "duration_ms": 30000}
        ],
        "outputs": [
            {"id": "youtube", "uri_host": "a.rtmp.youtube.com", "state": "live",
             "reconnects": 0, "queue_secs": 0.2}
        ],
        "multiview": {"enabled": false, "width": 960, "height": 540, "cols": 2,
                      "rows": 1, "fps": 8, "cells": []},
        "uptime_secs": 61,
        "running_time_ms": 61000,
        "backend": {"video_decoder": "avdec_h264", "video_encoder": "x264enc",
                    "audio_decoder": "avdec_aac", "audio_encoder": "avenc_aac",
                    "hardware_accelerated": false},
        "program": null
    })
}

/// The UI, wired to a fake core, with no terminal anywhere.
pub struct Harness {
    pub app: App,
    pub fake: Fake,
    commands: mpsc::Sender<Command>,
    incoming: mpsc::Receiver<Incoming>,
}

impl Harness {
    pub async fn start(fake: Fake, multiview: Option<client::MultiviewWant>) -> Self {
        let config = Config { url: fake.url(), token: None, multiview };
        let (command_tx, command_rx) = mpsc::channel(32);
        let (incoming_tx, incoming_rx) = mpsc::channel(256);
        let app = App::new(&config);
        tokio::spawn(client::run(config, command_rx, incoming_tx));
        Self { app, fake, commands: command_tx, incoming: incoming_rx }
    }

    /// Feed the app whatever the link has, until `done` says so or the clock
    /// runs out. Answers false on a timeout, so a test fails on its own
    /// assertion rather than hanging.
    pub async fn pump_until(&mut self, done: impl Fn(&App) -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !done(&self.app) {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                return false;
            }
            match tokio::time::timeout(left, self.incoming.recv()).await {
                Ok(Some(message)) => {
                    if let Some(command) = self.app.on_incoming(message) {
                        let _ = self.commands.send(command).await;
                    }
                }
                Ok(None) => return false,
                Err(_) => return false,
            }
        }
        true
    }

    /// Drain the link for a while, so background work (a reconnect, a batch of
    /// deltas) reaches the app.
    pub async fn pump_ms(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                return;
            }
            match tokio::time::timeout(left, self.incoming.recv()).await {
                Ok(Some(message)) => {
                    if let Some(command) = self.app.on_incoming(message) {
                        let _ = self.commands.send(command).await;
                    }
                }
                _ => return,
            }
        }
    }

    /// One key, and the call it makes, sent the way the real loop sends it.
    pub async fn press(&mut self, key: char) {
        let event = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(key),
            crossterm::event::KeyModifiers::NONE,
        );
        let action = match self.app.mode {
            godwinmix_tui::app::Mode::Filter | godwinmix_tui::app::Mode::AdUri => {
                godwinmix_tui::keys::typing(event)
            }
            _ => godwinmix_tui::keys::normal(event),
        };
        if let Some(command) = self.app.act(action) {
            self.commands.send(command).await.expect("the link is listening");
        }
    }

    /// Enter, which accepts whatever is being typed.
    pub async fn press_enter(&mut self) {
        let event = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        let action = match self.app.mode {
            godwinmix_tui::app::Mode::Filter | godwinmix_tui::app::Mode::AdUri => {
                godwinmix_tui::keys::typing(event)
            }
            _ => godwinmix_tui::keys::normal(event),
        };
        if let Some(command) = self.app.act(action) {
            self.commands.send(command).await.expect("the link is listening");
        }
    }

    /// Wait until the fake core has seen a call to this method.
    pub async fn wait_for_call(&mut self, method: &str) -> Value {
        for _ in 0..500 {
            if let Some(call) = self.fake.calls(method).first().cloned() {
                return call;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the fake core never saw {method}");
    }
}
