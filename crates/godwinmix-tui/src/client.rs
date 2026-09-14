//! The link to the core: one WebSocket at `/rpc`, and nothing private about
//! it.
//!
//! Every method this UI calls is a method any other client can call, and the
//! subscription is the one from 05 section 2: `events: ["*"]`, and an `ext`
//! table that asks for meters, tally and positions. The mosaic is asked for
//! only when the operator passed `--multiview`, because nothing expensive runs
//! in the core unless a client asks for it.

use crate::model::RpcError;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

/// What the mosaic is asked for, when it is asked for at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiviewWant {
    pub fps: u32,
    pub width: u32,
}

impl Default for MultiviewWant {
    /// Four frames a second at 320 pixels: the numbers in 05 section 5, and
    /// about as much as a terminal can show without becoming the load.
    fn default() -> Self {
        Self { fps: 4, width: 320 }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Already normalised to `ws://host:port/rpc`.
    pub url: String,
    pub token: Option<String>,
    /// `None` means no `ext.multiview` key is sent at all.
    pub multiview: Option<MultiviewWant>,
}

/// What the UI asks the link to do.
#[derive(Debug, Clone)]
pub enum Command {
    /// One JSON-RPC call. The method comes back on the answer so the footer
    /// can say which call was refused.
    Call { method: String, params: Value },
    /// Subscribe again, after `event/resync`.
    Resubscribe,
}

/// What the link tells the UI.
#[derive(Debug, Clone)]
pub enum Incoming {
    Connected { url: String },
    Subscribed { seq: u64, ignored_ext: Vec<String> },
    Event { method: String, params: Value },
    Frame(Frame),
    CallOk { method: String, result: Value },
    CallErr { method: String, error: RpcError },
    Disconnected { reason: String, retry_in: Duration },
}

/// One mosaic frame: the 16 byte header from `protocol.md`, then the JPEG.
#[derive(Debug, Clone)]
pub struct Frame {
    pub seq: u32,
    pub layout: u32,
    pub running_time_ms: u64,
    pub jpeg: Vec<u8>,
}

impl Frame {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 16 {
            return None;
        }
        let seq = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let layout = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
        let running_time_ms = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
        Some(Self { seq, layout, running_time_ms, jpeg: bytes[16..].to_vec() })
    }
}

/// The `core.subscribe` params, built in one place so a test can read them.
///
/// `events: ["*"]` takes the lot; the `ext` table is what actually costs the
/// core anything, and the multiview key is absent unless it was asked for.
pub fn subscribe_params(config: &Config) -> Value {
    let mut ext = serde_json::Map::new();
    ext.insert("meters".into(), json!(true));
    ext.insert("tally".into(), json!(true));
    ext.insert("positions".into(), json!(true));
    if let Some(mv) = config.multiview {
        ext.insert("multiview".into(), json!({ "fps": mv.fps, "width": mv.width }));
    }
    json!({ "events": ["*"], "ext": Value::Object(ext) })
}

/// `http://host:8080` or `host:8080` or `ws://host/rpc` all mean the same
/// mixer. One place turns them into the URL the socket opens.
pub fn normalise_url(raw: &str) -> String {
    let raw = raw.trim();
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("ws://{raw}")
    };
    let with_scheme = if let Some(rest) = with_scheme.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if let Some(rest) = with_scheme.strip_prefix("https://") {
        format!("wss://{rest}")
    } else {
        with_scheme
    };
    let (base, query) = match with_scheme.split_once('?') {
        Some((b, q)) => (b.to_string(), format!("?{q}")),
        None => (with_scheme, String::new()),
    };
    let trimmed = base.trim_end_matches('/');
    let path_start = trimmed.find("://").map(|i| i + 3).unwrap_or(0);
    let has_path = trimmed[path_start..].contains('/');
    if has_path {
        format!("{trimmed}{query}")
    } else {
        format!("{trimmed}/rpc{query}")
    }
}

/// Backoff between reconnect attempts: half a second, doubling to fifteen.
fn backoff(attempt: u32) -> Duration {
    let ms = 500u64.saturating_mul(1 << attempt.min(5));
    Duration::from_millis(ms.min(15_000))
}

/// Run the link until the command channel closes. Reconnects on its own and
/// says so on the way past, so the screen can show the operator that the
/// mixer is not there rather than showing stale numbers as if they were live.
pub async fn run(config: Config, mut commands: mpsc::Receiver<Command>, out: mpsc::Sender<Incoming>) {
    let mut attempt = 0u32;
    loop {
        match session(&config, &mut commands, &out).await {
            SessionEnd::Closed => return,
            SessionEnd::Lost(reason) => {
                let wait = backoff(attempt);
                attempt = attempt.saturating_add(1);
                if out.send(Incoming::Disconnected { reason, retry_in: wait }).await.is_err() {
                    return;
                }
                tokio::time::sleep(wait).await;
            }
        }
    }
}

enum SessionEnd {
    /// The UI dropped the command channel: time to stop.
    Closed,
    /// The socket went away.
    Lost(String),
}

async fn session(
    config: &Config,
    commands: &mut mpsc::Receiver<Command>,
    out: &mpsc::Sender<Incoming>,
) -> SessionEnd {
    let request = match build_request(config) {
        Ok(r) => r,
        Err(e) => return SessionEnd::Lost(e),
    };
    let socket = match tokio_tungstenite::connect_async(request).await {
        Ok((socket, _)) => socket,
        Err(e) => return SessionEnd::Lost(format!("{e}")),
    };
    if out.send(Incoming::Connected { url: config.url.clone() }).await.is_err() {
        return SessionEnd::Closed;
    }
    let (mut tx, mut rx) = socket.split();
    let mut pending: HashMap<u64, String> = HashMap::new();
    let mut next_id = 1u64;
    if let Err(e) = send_subscribe(&mut tx, config, &mut next_id, &mut pending).await {
        return SessionEnd::Lost(e);
    }
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                None => return SessionEnd::Closed,
                Some(Command::Resubscribe) => {
                    if let Err(e) = send_subscribe(&mut tx, config, &mut next_id, &mut pending).await {
                        return SessionEnd::Lost(e);
                    }
                }
                Some(Command::Call { method, params }) => {
                    let id = next_id;
                    next_id += 1;
                    pending.insert(id, method.clone());
                    let frame = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
                    if let Err(e) = tx.send(Message::Text(frame.to_string().into())).await {
                        return SessionEnd::Lost(format!("{e}"));
                    }
                }
            },
            message = rx.next() => match message {
                None => return SessionEnd::Lost("the mixer closed the link".into()),
                Some(Err(e)) => return SessionEnd::Lost(format!("{e}")),
                Some(Ok(message)) => {
                    match on_message(message, &mut pending, out).await {
                        Ok(true) => {}
                        Ok(false) => return SessionEnd::Closed,
                        Err(reason) => return SessionEnd::Lost(reason),
                    }
                }
            },
        }
    }
}

type Sink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    Message,
>;

async fn send_subscribe(
    tx: &mut Sink,
    config: &Config,
    next_id: &mut u64,
    pending: &mut HashMap<u64, String>,
) -> Result<(), String> {
    let id = *next_id;
    *next_id += 1;
    pending.insert(id, "core.subscribe".to_string());
    let frame = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "core.subscribe",
        "params": subscribe_params(config),
    });
    tx.send(Message::Text(frame.to_string().into())).await.map_err(|e| format!("{e}"))
}

/// Returns Ok(false) when the UI has gone away.
async fn on_message(
    message: Message,
    pending: &mut HashMap<u64, String>,
    out: &mpsc::Sender<Incoming>,
) -> Result<bool, String> {
    let sent = match message {
        Message::Text(text) => {
            let Ok(value) = serde_json::from_str::<Value>(&text) else { return Ok(true) };
            match classify(&value, pending) {
                Some(incoming) => out.send(incoming).await,
                None => return Ok(true),
            }
        }
        Message::Binary(bytes) => match Frame::parse(&bytes) {
            Some(frame) => out.send(Incoming::Frame(frame)).await,
            None => return Ok(true),
        },
        Message::Close(_) => return Err("the mixer closed the link".into()),
        _ => return Ok(true),
    };
    Ok(sent.is_ok())
}

/// One text frame from the core: a notification, an answer, or a refusal.
fn classify(value: &Value, pending: &mut HashMap<u64, String>) -> Option<Incoming> {
    if let Some(method) = value.get("method").and_then(Value::as_str) {
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        return Some(Incoming::Event { method: method.to_string(), params });
    }
    let id = value.get("id").and_then(Value::as_u64)?;
    let method = pending.remove(&id).unwrap_or_else(|| "unknown".to_string());
    if let Some(error) = value.get("error") {
        return Some(Incoming::CallErr { method, error: read_error(error) });
    }
    let result = value.get("result").cloned().unwrap_or(Value::Null);
    if method == "core.subscribe" {
        let seq = result.get("seq").and_then(Value::as_u64).unwrap_or(0);
        let ignored_ext = result
            .get("ignored_ext")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        return Some(Incoming::Subscribed { seq, ignored_ext });
    }
    Some(Incoming::CallOk { method, result })
}

fn read_error(error: &Value) -> RpcError {
    RpcError {
        code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
        message: error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the mixer refused the call and said nothing about why")
            .to_string(),
        retryable: error.get("data").and_then(|d| d.get("retryable")).and_then(Value::as_bool),
    }
}

fn build_request(
    config: &Config,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, String> {
    let mut request = config.url.as_str().into_client_request().map_err(|e| format!("{e}"))?;
    if let Some(token) = &config.token {
        // A header, not `?token=`: a token in a URL ends up in more logs than
        // it should. The core takes either.
        let value = format!("Bearer {token}").parse().map_err(|_| {
            "the token has characters an HTTP header cannot carry. Use a printable ASCII token."
                .to_string()
        })?;
        request.headers_mut().insert("Authorization", value);
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_end_up_at_rpc() {
        assert_eq!(normalise_url("localhost:8080"), "ws://localhost:8080/rpc");
        assert_eq!(normalise_url("http://localhost:8080"), "ws://localhost:8080/rpc");
        assert_eq!(normalise_url("http://localhost:8080/"), "ws://localhost:8080/rpc");
        assert_eq!(normalise_url("https://mix.example"), "wss://mix.example/rpc");
        assert_eq!(normalise_url("ws://box/rpc"), "ws://box/rpc");
        assert_eq!(normalise_url("http://box:8080?token=x"), "ws://box:8080/rpc?token=x");
    }

    #[test]
    fn no_multiview_key_unless_asked() {
        let config = Config { url: "ws://x/rpc".into(), token: None, multiview: None };
        let params = subscribe_params(&config);
        let ext = params.get("ext").unwrap().as_object().unwrap();
        assert!(!ext.contains_key("multiview"), "ext carried a multiview key: {ext:?}");
        assert_eq!(ext.get("meters"), Some(&json!(true)));
        assert_eq!(ext.get("tally"), Some(&json!(true)));
        assert_eq!(ext.get("positions"), Some(&json!(true)));
    }

    #[test]
    fn multiview_carries_fps_and_width() {
        let config = Config {
            url: "ws://x/rpc".into(),
            token: None,
            multiview: Some(MultiviewWant::default()),
        };
        let params = subscribe_params(&config);
        assert_eq!(params["ext"]["multiview"], json!({"fps": 4, "width": 320}));
    }

    #[test]
    fn backoff_climbs_then_stops() {
        assert_eq!(backoff(0), Duration::from_millis(500));
        assert_eq!(backoff(3), Duration::from_millis(4000));
        assert_eq!(backoff(9), Duration::from_millis(15_000));
    }

    #[test]
    fn a_frame_is_a_header_then_a_jpeg() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&7u32.to_le_bytes());
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&1234u64.to_le_bytes());
        bytes.extend_from_slice(b"jpeg");
        let frame = Frame::parse(&bytes).unwrap();
        assert_eq!((frame.seq, frame.layout, frame.running_time_ms), (7, 3, 1234));
        assert_eq!(frame.jpeg, b"jpeg");
    }
}
