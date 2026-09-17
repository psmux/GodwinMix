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

use godwinmix_protocol::rpc::{self, MeterBatch, Subscription};
use godwinmix_protocol::scope::Token;
use godwinmix_protocol::{Flush, Resync, Snapshot, SubscribeRequest, SubscribeResult, Tally};
use godwinmix_protocol::types::Event;
use crate::control::call::dispatch;
use crate::control::{Ctx, RunningTime};
use godwinmix_core::multiview::{MultiviewRequest, MultiviewSubscription};
use godwinmix_core::state::Envelope;
use axum::extract::ws::{Message, WebSocket};
use futures_util::stream::{SplitSink, StreamExt};
use futures_util::SinkExt;
use serde_json::{json, Map, Value};
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, warn};

type Sink = SplitSink<WebSocket, Message>;

/// How long one frame may take to reach a client before the connection is
/// given up on.
///
/// Every send in this file goes through it. A peer that stops reading, a laptop
/// that sleeps with the lid shut, a proxy that keeps the socket open and
/// forwards nothing: in all three the send future simply never completes, and
/// this task owns the `MultiviewSubscription` that keeps the mosaic running.
/// One dead client therefore used to hold the mosaic encoder up for everybody,
/// for as long as the TCP connection survived, which on a LAN is minutes.
///
/// Five seconds is far longer than any real send on any link worth serving, and
/// far shorter than a mosaic anybody is paying for. On expiry the connection
/// ends, the subscription drops, and the pipeline goes after its linger.
const SEND_DEADLINE: Duration = Duration::from_secs(5);

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
    /// The scene on air, when one is. The mixer names a source instead when
    /// the programme is a single source, so tally needs both.
    program_scene: Option<String>,
    sources: Vec<String>,
    /// The layout id this client was last told about. Every mosaic frame
    /// carries one, so a grid that changes under a client has to be announced
    /// or the frames stop matching anything it knows.
    layout: u32,
    /// The programme clock, carried forward between status snapshots so every
    /// frame header this connection writes has a running time on it.
    clock: RunningTime,
    /// Frames written to this client, which is the counter in the header.
    frame_no: u32,
    /// What this client asked the mosaic for, or None when it asked for no
    /// mosaic at all. `serve_rpc` turns a change here into a subscription.
    wants_mosaic: Option<MultiviewRequest>,
    /// Whether this client asked for the preview scene. Decides whether
    /// `event/tally` carries `preview` and whether the layout says the preview
    /// is empty.
    wants_preview: bool,
    /// What keeps the preview compositor up while this client wants one.
    /// Dropped with the connection, which is what takes it away again.
    preview: Option<godwinmix_core::multiview::PreviewSubscription>,
    /// `ext.telemetry` and `ext.agent`. Holding this is what keeps the
    /// telemetry probes measuring. See `control/push.rs`.
    push: crate::control::push::Push,
}

pub async fn serve_rpc(socket: WebSocket, ctx: Ctx, token: Token) {
    let (tx, mut rx) = socket.split();
    let mut events = ctx.app.mixer.subscribe();
    let mut conn = Connection {
        tx,
        ctx,
        token,
        sub: None,
        meters: MeterBatch::default(),
        seq: 0,
        program: None,
        program_scene: None,
        sources: Vec::new(),
        layout: 0,
        clock: RunningTime::default(),
        frame_no: 0,
        wants_mosaic: None,
        wants_preview: false,
        preview: None,
        push: crate::control::push::Push::none(),
    };
    // Holding this is what keeps the mosaic up, and dropping it is what takes
    // it down again after the linger. A client that never asks for
    // `ext.multiview` never builds one. See `multiview.rs`.
    let mut mosaic: Option<MultiviewSubscription> = None;
    let mut asked: Option<MultiviewRequest> = None;
    // The scene document's own change stream. Taken for every connection,
    // whether or not it has subscribed yet: the channel is a broadcast and a
    // receiver that nobody reads from costs one slot, while starting it later
    // would mean missing the patches raised between subscribing and the next
    // poll. Nothing is written to a client that did not ask for `scene.*`.
    let mut patches = conn.ctx.app.scenes.subscribe();

    loop {
        tokio::select! {
            incoming = rx.next() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Text(text))) => {
                    if conn.on_text(&text).await.is_err() {
                        break;
                    }
                    // A `core.subscribe` may have changed what this client
                    // wants out of the mosaic. The new subscription is taken
                    // before the old one is dropped, so re-subscribing never
                    // looks like the last viewer leaving.
                    if conn.wants_mosaic != asked {
                        asked = conn.wants_mosaic;
                        mosaic = asked.map(|req| conn.ctx.app.multiview.subscribe(req));
                    }
                }
                Some(Ok(_)) => {}
            },
            // `ext.telemetry` and `ext.agent`, on their own clock. Pending
            // for ever while neither was asked for, so a client that wanted
            // neither never wakes this task. See `control/push.rs`.
            _ = conn.push.due() => {
                if conn.send_push().await.is_err() {
                    break;
                }
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
            patch = patches.recv() => match patch {
                Ok(patch) => {
                    if conn.send_patch(&patch).await.is_err() {
                        break;
                    }
                    // A transaction commits as one patch, but a burst of
                    // separate edits (a drag, a multi select align) arrives as
                    // several. Draining and flushing once paints them as one
                    // update, which is the rule every other batch follows.
                    while let Ok(more) = patches.try_recv() {
                        if conn.send_patch(&more).await.is_err() {
                            return;
                        }
                    }
                    if conn.flush().await.is_err() {
                        break;
                    }
                }
                // A client that has fallen this far behind the document cannot
                // patch its way back, so it is told to fetch it again.
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    if conn.on_lag(n).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            frame = async {
                match mosaic.as_mut() {
                    Some(m) => m.recv().await,
                    None => std::future::pending().await,
                }
            } => match frame {
                Ok(jpeg) => {
                    if conn.send_frame(&jpeg).await.is_err() {
                        break;
                    }
                }
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

    /// One mosaic frame out, with the 16 byte header in front of it.
    ///
    /// The header is built per connection rather than once for everybody,
    /// because the layout id in it is the one this client was told about and
    /// two clients can be a grid apart.
    async fn send_frame(&mut self, jpeg: &[u8]) -> Result<(), ()> {
        if !self.wants_frames() {
            return Ok(());
        }
        self.frame_no = self.frame_no.wrapping_add(1);
        let header = rpc::frame_header(self.frame_no, self.layout, self.clock.now_ms());
        let mut out = Vec::with_capacity(header.len() + jpeg.len());
        out.extend_from_slice(&header);
        out.extend_from_slice(jpeg);
        self.write(Message::Binary(out.into())).await
    }

    async fn send(&mut self, value: Value) -> Result<(), ()> {
        let text = serde_json::to_string(&value).map_err(|_| ())?;
        self.write(Message::Text(text.into())).await
    }

    /// The one place this connection writes to its socket, so the deadline
    /// cannot be forgotten on a path added later.
    async fn write(&mut self, message: Message) -> Result<(), ()> {
        match tokio::time::timeout(SEND_DEADLINE, self.tx.send(message)).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(()),
            Err(_) => {
                warn!(
                    secs = SEND_DEADLINE.as_secs(),
                    "rpc client stopped reading; closing it so it stops holding the mosaic up"
                );
                Err(())
            }
        }
    }

    /// One text frame in: a request, a notification, or nonsense.
    async fn on_text(&mut self, text: &str) -> Result<(), ()> {
        let request = match rpc::parse(text) {
            Ok(r) => r,
            Err(bad) => {
                let trace = godwinmix_protocol::trace::new_id();
                return self.send(rpc::error_frame(&bad.id, &bad.error, &trace)).await;
            }
        };
        let id = godwinmix_protocol::trace::incoming(
            None,
            request.params.get("trace_id").and_then(Value::as_str),
        );
        let trace_id = id.to_string();
        // Subscribing changes this connection rather than the mixer, so it is
        // answered here instead of going through the method table.
        if request.method == "core.subscribe" {
            let result = self.subscribe(&request.params).await?;
            let Some(id) = request.id else { return Ok(()) };
            self.send(rpc::result_frame(&id, result)).await?;
            return self.after_subscribe().await;
        }
        // Inside the task local, so the call's log lines carry the same id the
        // client is holding, exactly as they do on /api/v1.
        let answer = godwinmix_core::observe::with_trace_id(
            id,
            dispatch(
                &self.ctx.registry,
                &self.ctx.app,
                &self.ctx.snapshots,
                &self.token,
                &trace_id,
                &request.method,
                request.params,
            ),
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
        // The preview scene is composited in the multiview pipeline (11
        // section 3), so asking for it is also asking for the mosaic. A client
        // that wants only the preview does not have to know that.
        let wants_multiview = request.ext.wants_multiview() || request.ext.wants_preview();
        self.wants_preview = request.ext.wants_preview();
        // The preview compositor is built by holding a subscription, exactly
        // as the mosaic is, and taken away when this connection drops it. The
        // armed scene is pushed first so the first frame is the right picture.
        self.preview = match self.wants_preview {
            true => {
                crate::control::push_preview(&self.ctx.app);
                let (fps, width) = match &request.ext.preview {
                    Some(godwinmix_protocol::PreviewExt::On { fps, width }) => {
                        (fps.unwrap_or(0) as i32, width.unwrap_or(0) as i32)
                    }
                    _ => (0, 0),
                };
                Some(self.ctx.app.multiview.subscribe_preview(
                    godwinmix_core::multiview::PreviewRequest {
                        fps,
                        width,
                        full: request.ext.wants_full_preview(),
                    },
                ))
            }
            false => None,
        };
        // What the mosaic is asked to run at. Zero on either means "whatever
        // is configured", which is what the clamp in multiview.rs reads it as.
        self.wants_mosaic = match (&request.ext.multiview, wants_multiview) {
            (Some(godwinmix_protocol::MultiviewExt::On { fps, width }), true) => Some(MultiviewRequest {
                fps: fps.unwrap_or(0) as i32,
                width: width.unwrap_or(0) as i32,
            }),
            (_, true) => Some(MultiviewRequest::configured()),
            _ => None,
        };
        let patterns =
            if request.events.is_empty() { vec!["*".to_string()] } else { request.events.clone() };
        self.push.configure(&request.ext);
        self.sub = Some(Subscription { patterns: patterns.clone(), ext: request.ext });
        self.seq = self.ctx.app.mixer.event_seq();
        // A client re-subscribing is rebuilding from nothing, so the grid it
        // was told about last time counts for nothing either.
        self.layout = 0;
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
        // Seed the frame clock from this read rather than waiting for the next
        // status event, so the first frame header carries a running time and a
        // layout that match what the client has just been sent.
        self.clock.observe(&Event::Status(Box::new(status.clone())));
        self.program = status.program.clone();
        self.program_scene = status.scene.clone();
        self.sources = status.sources.iter().map(|s| s.id.clone()).collect();
        let snapshot = Snapshot { seq: self.seq, state: Box::new(status.clone()) };
        self.send(rpc::notification(
            "event/snapshot",
            serde_json::to_value(snapshot).map_err(|_| ())?,
        ))
        .await?;
        self.send_layout(&status.multiview).await?;
        self.send_tally().await?;
        self.flush().await
    }

    /// Tell the client about the grid, when it wants the mosaic and the grid
    /// is not the one it already has.
    async fn send_layout(&mut self, multiview: &godwinmix_protocol::MultiviewStatus) -> Result<(), ()> {
        if !self.sub.as_ref().is_some_and(|s| s.wants("multiview.layout")) {
            return Ok(());
        }
        let layout = crate::control::layout_of(multiview);
        if layout.id == self.layout {
            return Ok(());
        }
        self.layout = layout.id;
        let mut value = serde_json::to_value(layout).map_err(|_| ())?;
        // A client that asked for the preview is told whether there is one. An
        // empty preview is a fact about the show, not an error, and saying so
        // is what stops a designer waiting for a picture that is not coming.
        if self.wants_preview {
            if let Some(map) = value.as_object_mut() {
                let sources = self.preview_sources();
                map.insert("preview_empty".into(), json!(sources.is_empty()));
                map.insert("preview_sources".into(), json!(sources));
                if sources.is_empty() {
                    map.insert(
                        "preview_note".into(),
                        json!(
                            "no scene is armed, so the preview is empty. Arm one with                              scene.preview.set when the scene server is available."
                        ),
                    );
                }
            }
        }
        self.send(rpc::notification("event/multiview.layout", value)).await
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
        // A full status is the snapshot event, not a delta. A source added or
        // removed reshuffles the mosaic, so the new grid goes out with it.
        if let Event::Status(status) = &envelope.event {
            let multiview = status.multiview.clone();
            let snapshot = Snapshot { seq: envelope.seq, state: status.clone() };
            let value = serde_json::to_value(snapshot).map_err(|_| ())?;
            self.send(rpc::notification("event/snapshot", value)).await?;
            return self.send_layout(&multiview).await;
        }
        let Some((name, mut payload)) = rpc::event_name_and_payload(&envelope.event) else {
            return Ok(());
        };
        // A take or an armed scene moves the tally whether or not this client
        // asked for the take event itself. A Stream Deck that subscribes to
        // `tally` alone must still see its lamps change.
        let moves_tally = name == "program.took" || name == "preview.changed";
        if sub.wants(name) {
            if let Some(map) = payload.as_object_mut() {
                map.insert("seq".into(), json!(envelope.seq));
            }
            self.send(rpc::notification(&format!("event/{name}"), payload)).await?;
        }
        if moves_tally {
            self.send_tally().await?;
        }
        Ok(())
    }

    /// Which sources the armed scene shows.
    ///
    /// The one body `event/tally`, `event/multiview.layout` and
    /// `/mjpeg/preview` all read, so a scene armed with `scene.preview.set`
    /// reaches every one of them at once. Empty when nothing is armed, which
    /// is a fact about the show and not an error.
    /// Every source the live scene draws.
    ///
    /// Empty when the programme is a single source, which the mixer names in
    /// `program` instead, and empty when nothing is on air. Without this the
    /// programme half of tally only ever lit the one source a scene of one
    /// item reports, so a camera in a two box was told it was off.
    fn program_sources(&self) -> Vec<String> {
        let Some(scene) = self.program_scene.as_deref() else { return Vec::new() };
        self.ctx.app.scenes.sources_in(scene)
    }

    fn preview_sources(&self) -> Vec<String> {
        let Some(layout) = self.ctx.app.scenes.preview_layout(1920, 1080) else {
            return Vec::new();
        };
        let mut sources: Vec<String> = layout
            .cells
            .into_iter()
            .filter(|c| c.alpha > 0.0)
            .map(|c| c.source)
            .collect();
        sources.dedup();
        sources
    }

    /// One change to the scene document, as `event/scene.patch`.
    ///
    /// Sent to a client that asked for `scene.*`, and to the client that made
    /// the change as well: it carries `source_client`, so the one that made it
    /// suppresses its own echo, and `client_seq`, so a drag it has already
    /// drawn past is discarded rather than snapping the handle backwards.
    async fn send_patch(&mut self, patch: &godwinmix_core::scene::server::Patch) -> Result<(), ()> {
        if !self.sub.as_ref().is_some_and(|s| s.wants("scene.patch")) {
            return Ok(());
        }
        let value = serde_json::to_value(patch).map_err(|_| ())?;
        self.seq = self.seq.saturating_add(1);
        self.send(rpc::notification("event/scene.patch", value)).await
    }

    /// Keep enough of the state to derive tally without asking the mixer.
    fn remember(&mut self, event: &Event) {
        self.clock.observe(event);
        match event {
            Event::Status(status) => {
                self.program = status.program.clone();
                self.program_scene = status.scene.clone();
                self.sources = status.sources.iter().map(|s| s.id.clone()).collect();
            }
            Event::Took { source, scene, .. } => {
                self.program = source.clone();
                self.program_scene = scene.clone();
            }
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
        // The armed scene's sources are `preview`, read off the scene server's
        // `preview_layout()`. With nothing armed, every source is `program` or
        // `off`; a client that needs to tell "nothing armed" from "armed and
        // empty" reads `preview_empty` on `event/multiview.layout`.
        let previewing = self.preview_sources();
        let on_air = self.program_sources();
        let mut sources = Map::new();
        for id in &self.sources {
            let state = if Some(id) == self.program.as_ref() || on_air.iter().any(|p| p == id) {
                "program"
            } else if previewing.iter().any(|p| p == id) {
                "preview"
            } else {
                "off"
            };
            sources.insert(id.clone(), Value::String(state.into()));
        }
        let mut value = serde_json::to_value(Tally { sources }).map_err(|_| ())?;
        if let Some(map) = value.as_object_mut() {
            map.insert("seq".into(), json!(self.seq));
        }
        self.send(rpc::notification("event/tally", value)).await
    }

    /// `event/telemetry` and `event/agent.state`, when this client asked for
    /// them and there is something to say.
    ///
    /// The liveness map comes from what this connection already remembers, so
    /// a tick at ten per second never asks the mixer for a status.
    async fn send_push(&mut self) -> Result<(), ()> {
        let live: std::collections::BTreeMap<String, u8> = self
            .sources
            .iter()
            .map(|id| (id.clone(), u8::from(Some(id) == self.program.as_ref())))
            .collect();
        let program = self.program.clone();
        let app = self.ctx.app.clone();
        let snapshots = self.ctx.snapshots.clone();
        let token = self.token.clone();
        // Only the agent document needs one, so a telemetry tick at ten per
        // second never touches the mixer's command queue.
        let status = match self.push.needs_status() {
            true => self.ctx.app.mixer.status().await.ok(),
            false => None,
        };
        let messages = self.push.messages(live, program.as_deref(), move || {
            let held = app.safety.check(&token).err().map(|r| r.message);
            match status {
                Some(status) => serde_json::to_value(
                    crate::control::methods::agent::document(&status, &snapshots, held),
                )
                .unwrap_or(Value::Null),
                None => Value::Null,
            }
        });
        for (name, mut payload) in messages {
            if let Some(map) = payload.as_object_mut() {
                map.insert("seq".into(), json!(self.seq));
            }
            self.send(rpc::notification(&format!("event/{name}"), payload)).await?;
        }
        Ok(())
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
        self.layout = 0;
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
            if write_legacy(&mut tx, Message::Text(json.into())).await.is_err() {
                return;
            }
        }
        Err(e) => {
            warn!(?e, "could not snapshot state for new websocket client");
            return;
        }
    }

    let mut events = ctx.app.mixer.subscribe();
    // The legacy stream always wanted every frame, so it holds a subscription
    // for as long as it is connected. That is what keeps the old UI working
    // while the mosaic only runs for clients that ask. A UI that asks for
    // nothing in particular gets the configured size.
    let mut frames = ctx.app.multiview.subscribe(MultiviewRequest::configured());

    loop {
        tokio::select! {
            incoming = rx.next() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },
            ev = events.recv() => match ev {
                Ok(ev) => {
                    let Ok(json) = serde_json::to_string(&ev.event) else { continue };
                    if write_legacy(&mut tx, Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(skipped = n, "websocket client fell behind on events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            // With multiview off this never yields, so the select just
            // handles events and no special case is needed.
            frame = frames.recv() => match frame {
                Ok(bytes) => {
                    if write_legacy(&mut tx, Message::Binary(bytes.to_vec().into()))
                        .await
                        .is_err()
                    {
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

/// The same deadline for the legacy socket, which holds a mosaic subscription
/// for its whole life and so can hold the pipeline up for everybody.
async fn write_legacy(tx: &mut Sink, message: Message) -> Result<(), ()> {
    match tokio::time::timeout(SEND_DEADLINE, tx.send(message)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(()),
        Err(_) => {
            warn!(
                secs = SEND_DEADLINE.as_secs(),
                "legacy websocket client stopped reading; closing it"
            );
            Err(())
        }
    }
}
