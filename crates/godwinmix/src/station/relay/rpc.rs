//! `/rpc` under a station. A client's frame is read once to see whose it is:
//! a station method is answered here, `core.subscribe` may move the
//! connection to another show, anything else goes to the show as written.
//! What the show sends comes back untouched, mosaic frames included, with
//! the station's own events (`show.*`, `channel.*`) written in between. When
//! the show's side closes the client's closes with 1012 and it reconnects
//! through the station, which waits for the show.

use super::super::methods;
use super::super::state::Station;
use super::pipe::{self, Upstream};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, Method, Uri};
use axum::response::Response;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use godwinmix_protocol::rpc::{self, Subscription};
use godwinmix_protocol::{error::RpcError, scope::Token, SubscribeRequest};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message as Up;
use tracing::debug;

mod station_side;
mod upstream;

pub async fn upgrade(st: Arc<Station>, ws: WebSocketUpgrade, uri: Uri, headers: HeaderMap) -> Response {
    let presented = crate::control::presented_token(&Method::GET, &headers, &uri);
    let token = match st.tokens.authenticate(presented.as_deref()) {
        Ok(t) => t,
        Err(reason) => {
            let e = RpcError::new(godwinmix_protocol::ErrorCode::Scope, format!("{}. Open /rpc?token=<token>, or send an Authorization header.", reason.message()));
            return super::http::refusal(&e);
        }
    };
    let show = super::show_in(uri.query()).unwrap_or_else(|| st.first());
    ws.on_upgrade(move |socket| async move {
        let relay = Relay { st, token, show, uri, headers, up: None, sub: None, seq: 0 };
        relay.serve(socket).await
    })
}

struct Relay {
    st: Arc<Station>,
    token: Token,
    show: String,
    uri: Uri,
    headers: HeaderMap,
    up: Option<SplitSink<Upstream, Up>>,
    /// For the station's events: what the client subscribed to.
    sub: Option<Subscription>,
    /// The last sequence number the show sent, for the flush after an event
    /// of the station's.
    seq: u64,
}

async fn next_up(rx: &mut Option<SplitStream<Upstream>>) -> Option<Up> {
    match rx.as_mut() {
        Some(rx) => rx.next().await.and_then(Result::ok),
        None => std::future::pending().await,
    }
}

impl Relay {
    async fn serve(mut self, socket: WebSocket) {
        let (mut c_tx, mut c_rx) = socket.split();
        let mut up_rx: Option<SplitStream<Upstream>> = None;
        let mut events = self.st.events.subscribe();
        let (answers, mut answered) = mpsc::channel::<Value>(16);
        let why = loop {
            tokio::select! {
                m = c_rx.next() => match m {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(reply) = self.client_frame(text.as_str(), &mut up_rx, &answers).await {
                            if send(&mut c_tx, reply).await.is_err() { break "the client stopped reading"; }
                        }
                    }
                    Some(Ok(Message::Binary(b))) => { self.send_show(Up::Binary(b)).await; }
                    Some(Ok(_)) => {}
                    _ => break "the client hung up",
                },
                m = next_up(&mut up_rx) => match m.and_then(pipe::down) {
                    Some(m) => {
                        if let Message::Text(t) = &m { self.note_seq(t.as_str()); }
                        if c_tx.send(m).await.is_err() { break "the client stopped reading"; }
                    }
                    None => {
                        let _ = c_tx.send(Message::Close(Some(CloseFrame { code: 1012, reason: "the show restarted or stopped".into() }))).await;
                        break "the show's side closed";
                    }
                },
                Some(reply) = answered.recv() => {
                    if send(&mut c_tx, reply).await.is_err() { break "the client stopped reading"; }
                }
                e = events.recv() => match e {
                    Ok(envelope) => {
                        for frame in self.station_event(&envelope.event) {
                            if send(&mut c_tx, frame).await.is_err() { break; }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break "the station is stopping",
                },
            }
        };
        if let Some(mut up) = self.up.take() {
            let _ = up.close().await;
        }
        debug!(client = %self.token.id, show = %self.show, why, "relayed rpc client disconnected");
    }

    /// One frame from the client. Answers with a frame to write back, when
    /// there is one to write now.
    async fn client_frame(&mut self, text: &str, up_rx: &mut Option<SplitStream<Upstream>>, answers: &mpsc::Sender<Value>) -> Option<Value> {
        let frame: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        let method = frame.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
        let params = frame.get("params").cloned().unwrap_or(Value::Null);
        if methods::answers(&method, &params) {
            self.answer_later(frame.get("id").cloned(), method, params, answers.clone());
            return None;
        }
        super::super::ingest::after(&self.st, &method);
        if method == "core.subscribe" {
            let req: SubscribeRequest = serde_json::from_value(params).unwrap_or_default();
            if let Some(show) = req.show.filter(|s| *s != self.show) {
                self.show = show;
                self.up = None;
                *up_rx = None;
            }
            let patterns = if req.events.is_empty() { vec!["*".to_string()] } else { req.events };
            self.sub = Some(Subscription { patterns, ext: req.ext });
        }
        if self.up.is_none() {
            match self.connect().await {
                Ok(up) => {
                    let (tx, rx) = up.split();
                    self.up = Some(tx);
                    *up_rx = Some(rx);
                }
                Err(e) => return frame.get("id").map(|id| rpc::error_frame(id, &e, &godwinmix_protocol::trace::new_id())),
            }
        }
        self.send_show(Up::Text(text.into())).await;
        None
    }
}

pub(super) async fn send(tx: &mut SplitSink<WebSocket, Message>, v: Value) -> Result<(), axum::Error> {
    tx.send(Message::Text(v.to_string().into())).await
}
