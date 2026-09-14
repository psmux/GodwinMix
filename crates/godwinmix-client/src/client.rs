//! The connection: one WebSocket, JSON-RPC over it, a store beside it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;

use crate::error::{Error, Result};
use crate::frames::parse_frame;
use crate::generated::{Event, SubscribeResult};
use crate::store::State;
use crate::urls;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;

/// A connection to one core.
///
/// Cheap to clone: every clone talks over the same socket and reads the same
/// store. Dropping the last clone closes the connection.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    out: mpsc::UnboundedSender<Message>,
    pending: Pending,
    next_id: AtomicU64,
    state: Arc<Mutex<State>>,
    events: broadcast::Sender<Event>,
    flushes: watch::Sender<u64>,
    base: String,
    token: Option<String>,
}

impl Client {
    /// Connect to a core and start reading its stream.
    ///
    /// `base` is the address of the mixer, `http://host:8080` or
    /// `https://...`; `ws://` and `wss://` are accepted too. Nothing is
    /// subscribed yet: call [`Client::subscribe`] to say what you want.
    pub async fn connect(base: &str, token: Option<&str>) -> Result<Client> {
        let url = urls::rpc(base, token);
        let (stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| Error::Transport(format!("{url}: {e}")))?;
        let (mut sink, mut source) = stream.split();

        let (out, mut outbox) = mpsc::unbounded_channel::<Message>();
        let (events, _) = broadcast::channel(256);
        let (flushes, _) = watch::channel(0u64);
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let state = Arc::new(Mutex::new(State { connected: true, ..State::default() }));

        let inner = Arc::new(Inner {
            out,
            pending: pending.clone(),
            next_id: AtomicU64::new(1),
            state: state.clone(),
            events: events.clone(),
            flushes: flushes.clone(),
            base: base.trim_end_matches('/').to_string(),
            token: token.map(str::to_string),
        });

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    outgoing = outbox.recv() => match outgoing {
                        Some(message) => {
                            if sink.send(message).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    },
                    incoming = source.next() => match incoming {
                        Some(Ok(message)) => {
                            handle(message, &pending, &state, &events, &flushes);
                        }
                        _ => break,
                    },
                }
            }
            // Whoever is waiting on a call gets told, rather than hanging.
            let waiting: Vec<_> = pending.lock().unwrap().drain().collect();
            for (_, tx) in waiting {
                let _ = tx.send(Err(Error::Closed));
            }
            state.lock().unwrap().connected = false;
        });

        Ok(Client { inner })
    }

    /// Send one call and wait for its answer.
    ///
    /// Every generated method goes through here, so every refusal has the one
    /// error shape.
    pub async fn call<P: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        method: &str,
        params: &P,
    ) -> Result<R> {
        let value = self.call_value(method, serde_json::to_value(params)?).await?;
        serde_json::from_value(value).map_err(|e| Error::Decode(format!("{method}: {e}")))
    }

    /// The same, without the typing, for a method a plugin added that this
    /// api_level has never heard of.
    pub async fn call_value(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.inner.pending.lock().unwrap().insert(id, tx);
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let text = serde_json::to_string(&body)?;
        if self.inner.out.send(Message::Text(text.into())).is_err() {
            self.inner.pending.lock().unwrap().remove(&id);
            return Err(Error::Closed);
        }
        match rx.await {
            Ok(answer) => answer,
            Err(_) => Err(Error::Closed),
        }
    }

    /// Ask for events, and for the expensive streams this surface wants.
    ///
    /// ```no_run
    /// # async fn go(client: godwinmix_client::Client) -> godwinmix_client::Result<()> {
    /// client.subscribe(&["program.*", "source.*", "flush"], serde_json::json!({"tally": true})).await?;
    /// # Ok(()) }
    /// ```
    pub async fn subscribe(&self, events: &[&str], ext: Value) -> Result<SubscribeResult> {
        self.call("core.subscribe", &json!({"events": events, "ext": ext})).await
    }

    /// A copy of the state as it stands. Read this at a flush, not per event.
    pub fn state(&self) -> State {
        self.inner.state.lock().unwrap().clone()
    }

    /// Every event, parsed. A receiver that falls behind is told so by the
    /// channel rather than being silently starved.
    pub fn events(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    /// The end of each batch, carrying the sequence number.
    ///
    /// This is the render loop: `while rx.changed().await.is_ok()` repaints
    /// once per batch, and a surface that falls behind coalesces rather than
    /// queueing up repaints it no longer needs.
    ///
    /// ```no_run
    /// # async fn go(client: godwinmix_client::Client) {
    /// let mut flushes = client.flushes();
    /// while flushes.changed().await.is_ok() {
    ///     let state = client.state();
    ///     println!("seq {} programme {:?}", state.seq, state.program());
    /// }
    /// # }
    /// ```
    pub fn flushes(&self) -> watch::Receiver<u64> {
        self.inner.flushes.subscribe()
    }

    /// Wait until the state is settled: the sequence number of a flush.
    ///
    /// Answers straight away when the core has already flushed once, which is
    /// what a script wants after `subscribe`. A render loop holds a
    /// [`Client::flushes`] receiver instead, so it does not miss batches.
    pub async fn next_flush(&self) -> Result<u64> {
        let mut rx = self.flushes();
        let seen = *rx.borrow_and_update();
        if seen > 0 {
            return Ok(seen);
        }
        rx.changed().await.map_err(|_| Error::Closed)?;
        let seq = *rx.borrow_and_update();
        Ok(seq)
    }

    /// `GET /api/v1/snapshot/{name}` for this core, token included.
    pub fn snapshot_url(&self, name: &str, width: Option<u32>) -> String {
        urls::snapshot(&self.inner.base, name, width, self.inner.token.as_deref())
    }

    /// `GET /mjpeg/{name}`, the multipart stream a non browser surface reads.
    pub fn mjpeg_url(&self, name: &str, width: Option<u32>) -> String {
        urls::mjpeg(&self.inner.base, name, width, self.inner.token.as_deref())
    }

    /// `POST /whep/{name}`, the WebRTC offer endpoint.
    pub fn whep_url(&self, name: &str) -> String {
        urls::whep(&self.inner.base, name, self.inner.token.as_deref())
    }

    /// Put a source on programme. `None` cuts to the slate.
    pub async fn take(&self, source: Option<&str>) -> Result<crate::generated::ProgramState> {
        self.call("program.take", &json!({"source": source})).await
    }

    /// Close the connection. Every clone stops with it.
    pub fn close(&self) {
        let _ = self.inner.out.send(Message::Close(None));
    }
}

/// One message in. Replies land on their waiting call, events land in the
/// store and then on the broadcast.
fn handle(
    message: Message,
    pending: &Pending,
    state: &Arc<Mutex<State>>,
    events: &broadcast::Sender<Event>,
    flushes: &watch::Sender<u64>,
) {
    match message {
        Message::Text(text) => {
            let Ok(value) = serde_json::from_str::<Value>(&text) else { return };
            if let Some(id) = value.get("id").and_then(Value::as_u64) {
                let waiting = pending.lock().unwrap().remove(&id);
                if let Some(tx) = waiting {
                    let _ = tx.send(answer(&value));
                    return;
                }
            }
            let Some(method) = value.get("method").and_then(Value::as_str) else { return };
            if !method.starts_with("event/") {
                return;
            }
            let params = value.get("params").cloned().unwrap_or(json!({}));
            let event = Event::parse(method, params);
            let flushed = state.lock().unwrap().apply(&event);
            let seq = if flushed { state.lock().unwrap().seq } else { 0 };
            let _ = events.send(event);
            if flushed {
                let _ = flushes.send_replace(seq);
            }
        }
        Message::Binary(bytes) => {
            if let Some(frame) = parse_frame(&bytes) {
                let _ = events.send(Event::MultiviewFrame(frame));
            }
        }
        _ => {}
    }
}

fn answer(value: &Value) -> Result<Value> {
    if let Some(error) = value.get("error") {
        return Err(Error::Rpc {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(-32603) as i32,
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("the call failed")
                .to_string(),
            data: error.get("data").cloned().unwrap_or(json!({})),
        });
    }
    Ok(value.get("result").cloned().unwrap_or(json!({})))
}
