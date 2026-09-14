//! `/rpc`, and the legacy `/ws` beside it for one release.
//!
//! One WebSocket carries everything: JSON-RPC text frames both ways, and
//! mosaic frames as binary frames with a 16 byte header. A client subscribes
//! with `core.subscribe`, is sent `event/snapshot` and then deltas, and every
//! batch ends with `event/flush` so it renders whole updates and never half
//! of one.
//!
//! The audit's three complaints about `/ws` are what this file answers: every
//! client got every event and every mosaic frame whether it wanted them or
//! not, there were no sequence numbers, and a client that fell behind was
//! told nothing.

use crate::api::rpc::{self, MeterBatch, Subscription};
use crate::api::scope::Token;
use crate::api::{Flush, Resync, Snapshot, SubscribeRequest, SubscribeResult, Tally};
use crate::api::types::Event;
use crate::control::call::dispatch;
use crate::control::Ctx;
use crate::state::Envelope;
use axum::extract::ws::{Message, WebSocket};
use futures_util::stream::{SplitSink, StreamExt};
use futures_util::SinkExt;
use serde_json::{json, Map, Value};
use tokio::sync::broadcast;
use tracing::{debug, warn};

type Sink = SplitSink<WebSocket, Message>;

/// One `/rpc` client.
struct Connection {
    tx: Sink,
    ctx: Ctx,
    token: Token,
    /// None until `core.subscribe` arrives. A client may call methods without
    /// ever subscribing, which is what the CLI and an agent do.
    sub: Option<Subscription>,
    /// Meters gathered across a batch, sent as one message at the flush.
    meters: MeterBatch,
    /// The last sequence number written to this client.
    seq: u64,
    /// What the client has been told is on air and what sources exist, so
    /// `event/tally` can be derived without asking the mixer every time.
    program: Option<String>,
    sources: Vec<String>,
    /// Held while this client wants the mosaic. Dropping it tells the gate the
    /// last viewer has gone.
    _multiview: Option<crate::api::streams::GateGuard>,
}

pub async fn serve_rpc(socket: WebSocket, ctx: Ctx, token: Token) {
    let (tx, mut rx) = socket.split();
    let mut events = ctx.app.mixer.subscribe();
    let mut frames = ctx.app.headed_frames.subscribe();
    let mut conn = Connection {
        tx,
        ctx,
        token,
        sub: None,
        meters: MeterBatch::default(),
        seq: 0,
        program: None,
        sources: Vec::new(),
        _multiview: None,
    };

    loop {
        tokio::select! {
            incoming = rx.next() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Text(text))) => {
                    if conn.on_text(&text).await.is_err() {
                        break;
                    }
                }
                Some(Ok(_)) => {}
            },
            event = events.recv() => match event {
                Ok(envelope) => {
                    if conn.on_event(envelope).await.is_err() {
                        break;
                    }
                    // Drain whatever else is waiting, then flush once, so a
                    // burst of changes paints as one update.
                    while let Ok(more) = events.try_recv() {
                        if conn.absorb(more).await.is_err() {
                            return;
                        }
                    }
                    if conn.flush().await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    if conn.on_lag(n).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            frame = frames.recv() => match frame {
                Ok(bytes) if conn.wants_frames() => {
                    if conn.tx.send(Message::Binary(bytes.to_vec().into())).await.is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                // A dropped preview frame is the right answer on a slow link:
                // the next one is along in an eighth of a second.
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(skipped = n, "rpc client fell behind on mosaic frames");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    debug!("rpc client disconnected");
}

impl Connection {
    fn wants_frames(&self) -> bool {
        self.sub.as_ref().is_some_and(|s| s.wants("multiview.frame"))
    }

    async fn send(&mut self, value: Value) -> Result<(), ()> {
        let text = serde_json::to_string(&value).map_err(|_| ())?;
        self.tx.send(Message::Text(text.into())).await.map_err(|_| ())
    }

    /// One text frame in: a request, a notification, or nonsense.
    async fn on_text(&mut self, text: &str) -> Result<(), ()> {
        let request = match rpc::parse(text) {
            Ok(r) => r,
            Err(bad) => {
                let trace = crate::api::trace::new_id();
                return self.send(rpc::error_frame(&bad.id, &bad.error, &trace)).await;
            }
        };
        let trace_id = crate::api::trace::from_parts(
            None,
            request.params.get("trace_id").and_then(Value::as_str),
        );
        // Subscribing changes this connection rather than the mixer, so it is
        // answered here instead of going through the method table.
        if request.method == "core.subscribe" {
            let result = self.subscribe(&request.params).await?;
            let Some(id) = request.id else { return Ok(()) };
            self.send(rpc::result_frame(&id, result)).await?;
            return self.after_subscribe().await;
        }
        let answer = dispatch(
            &self.ctx.registry,
            &self.ctx.app,
            &self.ctx.snapshots,
            &self.token,
            &trace_id,
            &request.method,
            request.params,
        )
        .await;
        let Some(id) = request.id else { return Ok(()) };
        match answer {
            Ok(mut value) => {
                if let Some(map) = value.as_object_mut() {
                    map.insert("trace_id".into(), Value::String(trace_id));
                }
                self.send(rpc::result_frame(&id, value)).await
            }
            Err(e) => self.send(rpc::error_frame(&id, &e, &trace_id)).await,
        }
    }

    /// Take up a subscription, and the mosaic pipeline with it when asked.
    async fn subscribe(&mut self, params: &Value) -> Result<Value, ()> {
        let request: SubscribeRequest = serde_json::from_value(params.clone()).unwrap_or_default();
        let ignored = request.ext.unsupported();
        let wants_multiview = request.ext.wants_multiview();
        let patterns =
            if request.events.is_empty() { vec!["*".to_string()] } else { request.events.clone() };
        self.sub = Some(Subscription { patterns: patterns.clone(), ext: request.ext });
        // The guard is taken after the old one is dropped, so re-subscribing
        // never looks like the last viewer leaving.
        self._multiview = None;
        if wants_multiview {
            self._multiview = Some(self.ctx.app.multiview_gate.subscribe());
        }
        self.seq = self.ctx.app.mixer.event_seq();
        serde_json::to_value(SubscribeResult {
            seq: self.seq,
            events: patterns,
            ignored_ext: ignored,
        })
        .map_err(|_| ())
    }

    /// The snapshot, the layout and a flush, in that order, so a client has a
    /// whole view before the first delta lands.
    async fn after_subscribe(&mut self) -> Result<(), ()> {
        let Ok(status) = self.ctx.app.mixer.status().await else { return Ok(()) };
        self.program = status.program.clone();
        self.sources = status.sources.iter().map(|s| s.id.clone()).collect();
        let snapshot = Snapshot { seq: self.seq, state: Box::new(status.clone()) };
        self.send(rpc::notification(
            "event/snapshot",
            serde_json::to_value(snapshot).map_err(|_| ())?,
        ))
        .await?;
        if self.sub.as_ref().is_some_and(|s| s.wants("multiview.layout")) {
            let layout = crate::control::layout_of(&status.multiview);
            self.send(rpc::notification(
                "event/multiview.layout",
                serde_json::to_value(layout).map_err(|_| ())?,
            ))
            .await?;
        }
        self.send_tally().await?;
        self.flush().await
    }

    /// One event: remember what it says, then write it out if this client
    /// asked for it.
    async fn on_event(&mut self, envelope: Envelope) -> Result<(), ()> {
        self.absorb(envelope).await
    }

    async fn absorb(&mut self, envelope: Envelope) -> Result<(), ()> {
        self.seq = envelope.seq;
        self.remember(&envelope.event);
        let Some(sub) = self.sub.as_ref() else { return Ok(()) };
        if self.meters.absorb(&envelope.event) {
            return Ok(());
        }
        // A full status is the snapshot event, not a delta.
        if let Event::Status(status) = &envelope.event {
            let snapshot = Snapshot { seq: envelope.seq, state: status.clone() };
            let value = serde_json::to_value(snapshot).map_err(|_| ())?;
            return self.send(rpc::notification("event/snapshot", value)).await;
        }
        let Some((name, mut payload)) = rpc::event_name_and_payload(&envelope.event) else {
            return Ok(());
        };
        if !sub.wants(name) {
            return Ok(());
        }
        if let Some(map) = payload.as_object_mut() {
            map.insert("seq".into(), json!(envelope.seq));
        }
        self.send(rpc::notification(&format!("event/{name}"), payload)).await?;
        if name == "program.took" {
            self.send_tally().await?;
        }
        Ok(())
    }

    /// Keep enough of the state to derive tally without asking the mixer.
    fn remember(&mut self, event: &Event) {
        match event {
            Event::Status(status) => {
                self.program = status.program.clone();
                self.sources = status.sources.iter().map(|s| s.id.clone()).collect();
            }
            Event::Took { source, .. } => self.program = source.clone(),
            Event::SourceStateChanged { source, .. }
                if !self.sources.iter().any(|s| s == source) =>
            {
                self.sources.push(source.clone());
            }
            _ => {}
        }
    }

    async fn send_tally(&mut self) -> Result<(), ()> {
        if !self.sub.as_ref().is_some_and(|s| s.wants("tally")) {
            return Ok(());
        }
        let mut sources = Map::new();
        for id in &self.sources {
            let state = if Some(id) == self.program.as_ref() { "program" } else { "off" };
            sources.insert(id.clone(), Value::String(state.into()));
        }
        let mut value = serde_json::to_value(Tally { sources }).map_err(|_| ())?;
        if let Some(map) = value.as_object_mut() {
            map.insert("seq".into(), json!(self.seq));
        }
        self.send(rpc::notification("event/tally", value)).await
    }

    /// End a batch: the meters gathered in it, then the flush marker a client
    /// renders on.
    async fn flush(&mut self) -> Result<(), ()> {
        if self.sub.is_none() {
            return Ok(());
        }
        if let Some(meters) = self.meters.take() {
            if self.sub.as_ref().is_some_and(|s| s.wants("meters")) {
                let mut value = serde_json::to_value(meters).map_err(|_| ())?;
                if let Some(map) = value.as_object_mut() {
                    map.insert("seq".into(), json!(self.seq));
                }
                self.send(rpc::notification("event/meters", value)).await?;
            }
        }
        let value = serde_json::to_value(Flush { seq: self.seq }).map_err(|_| ())?;
        self.send(rpc::notification("event/flush", value)).await
    }

    /// This client fell behind. Say so with the last number it is known to
    /// have, then send it a fresh snapshot rather than leaving it to guess.
    async fn on_lag(&mut self, dropped: u64) -> Result<(), ()> {
        warn!(dropped, "rpc client fell behind on events, resyncing");
        if self.sub.is_none() {
            return Ok(());
        }
        let value =
            serde_json::to_value(Resync { from_seq: self.seq, dropped }).map_err(|_| ())?;
        self.send(rpc::notification("event/resync", value)).await?;
        self.seq = self.ctx.app.mixer.event_seq();
        self.after_subscribe().await
    }
}

/// The legacy `/ws`: a broadcast of everything to everyone, exactly as it
/// was, for the UI and the Python example that have not moved yet.
pub async fn serve_legacy(socket: WebSocket, ctx: Ctx) {
    let (mut tx, mut rx) = socket.split();
    match ctx.app.mixer.status().await {
        Ok(s) => {
            let ev = Event::Status(Box::new(s));
            let Ok(json) = serde_json::to_string(&ev) else { return };
            if tx.send(Message::Text(json.into())).await.is_err() {
                return;
            }
        }
        Err(e) => {
            warn!(?e, "could not snapshot state for new websocket client");
            return;
        }
    }

    let mut events = ctx.app.mixer.subscribe();
    // The legacy stream always wanted every frame, so it holds a place in the
    // gate for as long as it is connected. That is what keeps the old UI
    // working while the mosaic only runs for clients that ask.
    let _watching = ctx.app.frames.is_some().then(|| ctx.app.multiview_gate.subscribe());
    let mut frames = ctx.app.frames.as_ref().map(|f| f.subscribe());

    loop {
        tokio::select! {
            incoming = rx.next() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },
            ev = events.recv() => match ev {
                Ok(ev) => {
                    let Ok(json) = serde_json::to_string(&ev.event) else { continue };
                    if tx.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(skipped = n, "websocket client fell behind on events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            frame = async {
                match frames.as_mut() {
                    Some(f) => f.recv().await,
                    None => std::future::pending().await,
                }
            } => match frame {
                Ok(bytes) => {
                    if tx.send(Message::Binary(bytes.to_vec().into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(skipped = n, "websocket client fell behind on preview frames");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    debug!("legacy websocket client disconnected");
}
