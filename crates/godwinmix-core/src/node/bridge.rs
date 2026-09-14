//! One JSON-RPC peer over one WebSocket, used identically by both ends.
//!
//! The core and a node are symmetric: either may call the other, either may
//! send a notification, and both keep their own request id space so the two
//! never collide. That symmetry is the point. Everything the core does to a
//! plugin over a sidecar's stdin, it does to a remote plugin through this,
//! with `instance` in the params saying which one.
//!
//! Nothing here touches GStreamer and nothing here blocks: a call returns a
//! future, a timeout answers rather than hanging, and a socket that dies
//! settles every call waiting on it with a reason instead of leaving them.

use super::wire::{Frame, FrameError, CALL_TIMEOUT_MS};
use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

/// What a peer does with an inbound request. Returning an error turns into a
/// JSON-RPC error frame; the far side sees a refusal, not a dropped call.
pub type Answer = Pin<Box<dyn Future<Output = Result<Value, FrameError>> + Send>>;
pub type Handler = Arc<dyn Fn(String, Value) -> Answer + Send + Sync>;

/// A handler that refuses everything, for a peer that only ever calls out.
pub fn refuse_all() -> Handler {
    Arc::new(|method: String, _| {
        Box::pin(async move {
            Err(FrameError {
                code: -32601,
                message: format!("this peer answers no methods, and `{method}` is one of them"),
                data: None,
            })
        })
    })
}

/// A live bridge.
pub struct Peer {
    out: mpsc::UnboundedSender<Message>,
    pending: Mutex<HashMap<i64, oneshot::Sender<Result<Value, FrameError>>>>,
    next: AtomicI64,
    closed: AtomicBool,
    /// Why it closed, for the log line and for `node.get`.
    reason: Mutex<Option<String>>,
}

impl Peer {
    /// Drive `stream` until it ends.
    ///
    /// Returns the peer and a future that finishes when the socket does. The
    /// caller awaits the future to know the connection is over; dropping the
    /// peer does not close the socket, because the core holds one of these per
    /// node and a reconciler tick must not be able to hang up on a camera.
    pub fn start<S>(stream: S, handler: Handler) -> (Arc<Peer>, impl Future<Output = String>)
    where
        S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
            + futures_util::Sink<Message>
            + Send
            + 'static,
        <S as futures_util::Sink<Message>>::Error: std::fmt::Display,
    {
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
        let peer = Arc::new(Peer {
            out: out_tx,
            pending: Mutex::new(HashMap::new()),
            next: AtomicI64::new(1),
            closed: AtomicBool::new(false),
            reason: Mutex::new(None),
        });
        let me = peer.clone();
        let pump = async move {
            let (mut sink, mut source) = stream.split();
            let why = loop {
                tokio::select! {
                    outgoing = out_rx.recv() => {
                        let Some(msg) = outgoing else { break "the peer was closed".to_string() };
                        if let Err(e) = sink.send(msg).await {
                            break format!("the socket would not take a frame: {e}");
                        }
                    }
                    incoming = source.next() => {
                        match incoming {
                            Some(Ok(Message::Text(text))) => me.on_text(&text, &handler),
                            Some(Ok(Message::Binary(_))) => {
                                // The bridge is text only. A binary frame is a
                                // peer that thinks this is the mosaic socket.
                                break "a binary frame arrived on the node bridge".to_string();
                            }
                            Some(Ok(Message::Close(_))) => break "the far side closed".to_string(),
                            Some(Ok(_)) => {}
                            Some(Err(e)) => break format!("the socket failed: {e}"),
                            None => break "the socket ended".to_string(),
                        }
                    }
                }
            };
            me.shut(&why);
            why
        };
        (peer, pump)
    }

    /// Call the far side and wait for its answer.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.call_within(method, params, Duration::from_millis(CALL_TIMEOUT_MS)).await
    }

    pub async fn call_within(
        &self,
        method: &str,
        params: Value,
        within: Duration,
    ) -> Result<Value> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(anyhow!("{}", self.closed_reason(method)));
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id, tx);
        let frame = Frame::request(id, method, params);
        if let Err(e) = self.send(&frame) {
            self.pending.lock().remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(within, rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(e))) => Err(anyhow!("{} ({})", e.message, e.code)),
            Ok(Err(_)) => Err(anyhow!("{}", self.closed_reason(method))),
            Err(_) => {
                self.pending.lock().remove(&id);
                Err(anyhow!(
                    "`{method}` got no answer from the far side within {} ms. The call was not \
                     cancelled: read the state back before retrying",
                    within.as_millis()
                ))
            }
        }
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.send(&Frame::notification(method, params))
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub fn reason(&self) -> Option<String> {
        self.reason.lock().clone()
    }

    /// Hang up, settling every call waiting on this socket.
    pub fn close(&self, why: &str) {
        let _ = self.out.send(Message::Close(None));
        self.shut(why);
    }

    fn shut(&self, why: &str) {
        if self.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        *self.reason.lock() = Some(why.to_string());
        let waiting: Vec<_> = self.pending.lock().drain().map(|(_, tx)| tx).collect();
        for tx in waiting {
            let _ = tx.send(Err(FrameError {
                code: -32010,
                message: format!("the bridge closed before the answer came back: {why}"),
                data: None,
            }));
        }
    }

    fn closed_reason(&self, method: &str) -> String {
        let why = self.reason.lock().clone().unwrap_or_else(|| "it closed".into());
        format!(
            "`{method}` cannot go anywhere: the bridge to this node is down ({why}). It comes \
             back on its own when the node reconnects; watch `node.get`"
        )
    }

    fn send(&self, frame: &Frame) -> Result<()> {
        let text = serde_json::to_string(frame)?;
        self.out
            .send(Message::Text(text.into()))
            .map_err(|_| anyhow!("the bridge writer has stopped"))
    }

    fn on_text(self: &Arc<Self>, text: &str, handler: &Handler) {
        let frame: Frame = match serde_json::from_str(text) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(?e, "a frame on the node bridge was not JSON-RPC");
                return;
            }
        };
        match (frame.method.clone(), frame.id) {
            // A response to something we sent.
            (None, Some(id)) => {
                let Some(tx) = self.pending.lock().remove(&id) else {
                    tracing::debug!(id, "an answer arrived for a call nobody is waiting on");
                    return;
                };
                let outcome = match frame.error {
                    Some(e) => Err(e),
                    None => Ok(frame.result.unwrap_or(Value::Null)),
                };
                let _ = tx.send(outcome);
            }
            // A request: answer it, off this task so a slow handler cannot
            // stop the socket being read.
            (Some(method), Some(id)) => {
                let params = frame.params.unwrap_or(Value::Null);
                let fut = handler(method, params);
                let me = self.clone();
                tokio::spawn(async move {
                    let frame = match fut.await {
                        Ok(value) => Frame::answer(id, value),
                        Err(e) => Frame {
                            jsonrpc: "2.0".into(),
                            id: Some(id),
                            method: None,
                            params: None,
                            result: None,
                            error: Some(e),
                        },
                    };
                    let _ = me.send(&frame);
                });
            }
            // A notification: nothing goes back.
            (Some(method), None) => {
                let params = frame.params.unwrap_or(Value::Null);
                let fut = handler(method, params);
                tokio::spawn(async move {
                    let _ = fut.await;
                });
            }
            (None, None) => {
                tracing::warn!("a frame on the node bridge had neither a method nor an id");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Two peers over an in memory duplex, which is the same code path a TLS
    /// socket takes minus the TLS.
    async fn pair(a: Handler, b: Handler) -> (Arc<Peer>, Arc<Peer>) {
        let (one, two) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(async move { tokio_tungstenite::accept_async(one).await.unwrap() });
        let client = tokio_tungstenite::client_async("ws://node/bridge", two).await.unwrap().0;
        let server = server.await.unwrap();
        let (pa, pump_a) = Peer::start(server, a);
        let (pb, pump_b) = Peer::start(client, b);
        tokio::spawn(pump_a);
        tokio::spawn(pump_b);
        (pa, pb)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_call_gets_its_answer_back() {
        let echo: Handler = Arc::new(|method: String, params: Value| {
            Box::pin(async move { Ok(json!({ "method": method, "params": params })) })
        });
        let (_core, node) = pair(echo, refuse_all()).await;
        let answer = node.call("health", json!({"instance": "cam1"})).await.unwrap();
        assert_eq!(answer["method"], "health");
        assert_eq!(answer["params"]["instance"], "cam1");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_refusal_comes_back_as_an_error_and_not_a_hang() {
        let (_core, node) = pair(refuse_all(), refuse_all()).await;
        let e = node.call("node.spawn", json!({})).await.unwrap_err();
        assert!(e.to_string().contains("-32601"), "got {e}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn closing_the_socket_settles_a_call_in_flight() {
        // A handler that never answers, so the call is genuinely in flight
        // when the socket goes.
        let silent: Handler = Arc::new(|_, _| {
            Box::pin(async move {
                futures_util::future::pending::<()>().await;
                unreachable!()
            })
        });
        let (core, node) = pair(silent, refuse_all()).await;
        let calling = tokio::spawn(async move { node.call("health", json!({})).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        core.close("the test pulled the cable");
        let outcome = calling.await.unwrap();
        let e = outcome.unwrap_err();
        assert!(
            e.to_string().contains("bridge") || e.to_string().contains("closed"),
            "a call in flight when the socket dies must fail with a reason, got {e}"
        );
    }
}
