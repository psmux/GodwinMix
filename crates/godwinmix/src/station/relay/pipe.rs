//! A WebSocket to a show, passed through frame for frame.
//!
//! Used for every WebSocket path but `/rpc`: the legacy `/ws` and the
//! monitoring streams. Nothing is parsed; a frame from either side is
//! written to the other as it arrives, and either side closing closes both.

use super::super::state::Station;
use super::http::upstream_url;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as Up;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::debug;

pub type Upstream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Open the same path on the show, with the caller's credential.
pub async fn connect(addr: SocketAddr, uri: &Uri, headers: &HeaderMap) -> Result<Upstream, String> {
    let url = upstream_url("ws", addr, uri.path(), uri.query());
    let mut request = url.as_str().into_client_request().map_err(|e| e.to_string())?;
    if let Some(auth) = headers.get(axum::http::header::AUTHORIZATION) {
        request.headers_mut().insert(axum::http::header::AUTHORIZATION, auth.clone());
    }
    let (ws, _) = tokio_tungstenite::connect_async(request).await.map_err(|e| e.to_string())?;
    Ok(ws)
}

pub fn down(m: Up) -> Option<Message> {
    Some(match m {
        Up::Text(t) => Message::Text(t.as_str().into()),
        Up::Binary(b) => Message::Binary(b),
        Up::Ping(p) => Message::Ping(p),
        Up::Pong(p) => Message::Pong(p),
        Up::Close(_) => return None,
        Up::Frame(_) => return None,
    })
}

pub fn up(m: Message) -> Option<Up> {
    Some(match m {
        Message::Text(t) => Up::Text(t.as_str().into()),
        Message::Binary(b) => Up::Binary(b),
        Message::Ping(p) => Up::Ping(p),
        Message::Pong(p) => Up::Pong(p),
        Message::Close(_) => return None,
    })
}

pub async fn upgrade(st: Arc<Station>, ws: WebSocketUpgrade, uri: Uri, headers: HeaderMap) -> Response {
    let show = super::show_in(uri.query()).unwrap_or_else(|| st.first());
    let addr = match st.addr_of(&show).await {
        Ok(a) => a,
        Err(e) => return super::http::refusal(&e),
    };
    let upstream = match connect(addr, &uri, &headers).await {
        Ok(u) => u,
        Err(e) => {
            let err = godwinmix_protocol::error::RpcError::internal(format!("show {show} would not open {}: {e}", uri.path()));
            return super::http::refusal(&err);
        }
    };
    ws.on_upgrade(move |client| pump(client, upstream))
}

async fn pump(client: WebSocket, upstream: Upstream) {
    let (mut c_tx, mut c_rx) = client.split();
    let (mut u_tx, mut u_rx) = upstream.split();
    let from_client = async {
        while let Some(Ok(m)) = c_rx.next().await {
            let Some(m) = up(m) else { break };
            if u_tx.send(m).await.is_err() {
                break;
            }
        }
        let _ = u_tx.close().await;
    };
    let from_show = async {
        while let Some(Ok(m)) = u_rx.next().await {
            let Some(m) = down(m) else { break };
            if c_tx.send(m).await.is_err() {
                break;
            }
        }
        let _ = c_tx.close().await;
    };
    tokio::select! {
        _ = from_client => {}
        _ = from_show => {}
    }
    debug!("a relayed WebSocket closed");
}
