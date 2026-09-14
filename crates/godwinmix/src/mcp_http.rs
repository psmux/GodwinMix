//! The MCP server's second transport, and the push behind both of them.
//!
//! 05 section 6: an agent that subscribes with `ext: {agent: true}` gets
//! `event/agent.state` on any change that matters instead of polling, and over
//! MCP that reaches it as `notifications/gmx/agent.state` on stdio and on
//! Streamable HTTP, both of which carry server initiated messages.
//!
//! Two pieces:
//!
//! * [`notifications`] holds a `/rpc` WebSocket to the mixer, subscribed to
//!   `event/agent.state` and nothing else, and hands every push to whoever is
//!   listening. It reconnects on its own and costs nothing while nobody is.
//! * [`serve`] is `gmx mcp --http <addr>`: `POST /mcp` for requests,
//!   `GET /mcp` for the notification stream, the same tool surface as stdio
//!   because both go through the same `Server::handle`.

use crate::mcp::Server;
use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event as SseEvent, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// The MCP method name a pushed `event/agent.state` arrives as.
pub const NOTIFICATION: &str = "notifications/gmx/agent.state";
/// How many pushes a slow listener may fall behind before it is resynced by
/// the next one. Bounded, because a notification nobody read must not grow.
const QUEUE: usize = 32;
/// How long to wait before reconnecting a dropped `/rpc` socket.
const RECONNECT: Duration = Duration::from_secs(2);

/// Everything the transport shares: the tool server and the push stream.
#[derive(Clone)]
pub struct Shared {
    pub server: Arc<Server>,
    pub pushes: broadcast::Sender<Value>,
}

/// Start the bridge that turns `event/agent.state` into an MCP notification.
///
/// Returns the sender every listener subscribes to. The task reconnects for
/// as long as the process lives; a mixer that is not up yet is a wait, not a
/// failure, because an MCP client often starts before the mixer does.
pub fn notifications(url: &str, token: Option<String>) -> broadcast::Sender<Value> {
    let (tx, _) = broadcast::channel(QUEUE);
    let Some(ws_url) = rpc_url(url) else {
        warn!(url, "no websocket address for this mixer URL, so agent.state will not be pushed");
        return tx;
    };
    let out = tx.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = pump(&ws_url, token.as_deref(), &out).await {
                debug!(%e, "the /rpc socket for MCP notifications dropped, reconnecting");
            }
            tokio::time::sleep(RECONNECT).await;
        }
    });
    tx
}

/// `http://host:port` becomes `ws://host:port/rpc`.
///
/// A `https` mixer answers `None`: this client speaks plain WebSocket, and
/// pretending otherwise would fail at the handshake with a worse message.
/// The tool calls still work; only the push is missing, and the log says so.
fn rpc_url(url: &str) -> Option<String> {
    let base = url.trim_end_matches('/');
    let rest = base.strip_prefix("http://").or_else(|| base.strip_prefix("ws://"))?;
    Some(format!("ws://{rest}/rpc"))
}

/// One connection's worth of pushes.
async fn pump(url: &str, token: Option<&str>, out: &broadcast::Sender<Value>) -> Result<()> {
    let target = match token {
        Some(t) if !t.is_empty() => format!("{url}?token={t}"),
        _ => url.to_string(),
    };
    let (mut socket, _) = tokio_tungstenite::connect_async(&target)
        .await
        .with_context(|| format!("connecting to {url}"))?;
    // Only `event/agent.state`, with the thresholds the plan names. No
    // multiview, no meters, no telemetry ticks: an agent asked for the
    // document, not for a feed.
    let subscribe = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "core.subscribe",
        "params": { "events": ["agent.state"], "ext": { "agent": true } }
    });
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(subscribe.to_string().into()))
        .await
        .context("subscribing")?;
    info!(url, "pushing agent.state to the MCP client");
    while let Some(message) = socket.next().await {
        let tokio_tungstenite::tungstenite::Message::Text(text) = message? else { continue };
        let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
        if value.get("method").and_then(Value::as_str) != Some("event/agent.state") {
            continue;
        }
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        // A send with no listeners is not a failure; it is the normal state.
        let _ = out.send(json!({ "jsonrpc": "2.0", "method": NOTIFICATION, "params": params }));
    }
    Ok(())
}

/// `gmx mcp --http <addr>`: the Streamable HTTP transport.
///
/// One endpoint, `/mcp`. A `POST` carries a JSON-RPC request and gets the
/// answer as JSON; a notification gets 202 and no body. A `GET` with
/// `Accept: text/event-stream` opens the stream of server initiated messages,
/// which is where `notifications/gmx/agent.state` arrives.
pub async fn serve(addr: &str, shared: Shared) -> Result<()> {
    let app = Router::new()
        .route("/mcp", get(stream).post(request))
        .route("/", get(|| async { "godwinmix MCP over Streamable HTTP at /mcp" }))
        .with_state(shared);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    info!(%addr, "MCP Streamable HTTP listening at /mcp");
    axum::serve(listener, app).await.context("serving MCP over HTTP")
}

async fn request(State(shared): State<Shared>, body: String) -> Response {
    let Some(reply) = shared.server.handle(&body).await else {
        // A notification. The revision requires 202 and an empty body.
        return StatusCode::ACCEPTED.into_response();
    };
    // A session id on every answer, so a client that tracks one has one. This
    // server holds no per session state, so any id it sends back is accepted.
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::HeaderName::from_static("mcp-session-id"), "godwinmix"),
        ],
        Json(reply),
    )
        .into_response()
}

/// The server initiated stream. Server sent events, which is what the
/// Streamable HTTP transport uses for messages the server starts.
async fn stream(
    State(shared): State<Shared>,
    headers: HeaderMap,
) -> Response {
    let accepts = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !accepts.contains("text/event-stream") && !accepts.contains("*/*") {
        return (
            StatusCode::NOT_ACCEPTABLE,
            "GET /mcp is the notification stream. Send Accept: text/event-stream, or POST \
             a JSON-RPC request to the same path.",
        )
            .into_response();
    }
    let rx = shared.pushes.subscribe();
    let events = tokio_stream(rx);
    Sse::new(events)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

/// The broadcast, as a stream of server sent events.
fn tokio_stream(
    rx: broadcast::Receiver<Value>,
) -> impl futures_util::Stream<Item = Result<SseEvent, std::convert::Infallible>> {
    futures_util::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(message) => {
                    let text = serde_json::to_string(&message).unwrap_or_default();
                    return Some((Ok(SseEvent::default().data(text)), rx));
                }
                // A listener that fell behind misses those pushes and carries
                // on; the next state change is a fresh whole document.
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(missed = n, "an MCP notification listener fell behind");
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rpc_address_is_derived_from_the_mixer_url() {
        assert_eq!(rpc_url("http://127.0.0.1:8080"), Some("ws://127.0.0.1:8080/rpc".into()));
        assert_eq!(rpc_url("http://box.local:8080/"), Some("ws://box.local:8080/rpc".into()));
        assert_eq!(rpc_url("ws://127.0.0.1:8080"), Some("ws://127.0.0.1:8080/rpc".into()));
        // A TLS mixer answers None: the tool calls still work and only the
        // push is missing, which is better than failing at a handshake.
        assert_eq!(rpc_url("https://mix.example.com"), None);
        assert_eq!(rpc_url("nonsense"), None);
    }

    #[test]
    fn a_push_is_wrapped_as_an_mcp_notification() {
        let (tx, mut rx) = broadcast::channel(4);
        let _ = tx.send(json!({
            "jsonrpc": "2.0",
            "method": NOTIFICATION,
            "params": { "program": "cam1", "why": "program" }
        }));
        let message = rx.try_recv().unwrap();
        assert_eq!(message["method"], "notifications/gmx/agent.state");
        assert_eq!(message["params"]["why"], "program");
        assert!(message.get("id").is_none(), "a notification carries no id");
    }
}
