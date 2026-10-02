//! Feeds that push: a websocket's messages and an event stream's events.
//!
//! Each message is a whole document. A server that sends fifty a second
//! does not cause fifty writes: the latest is kept and written at most five
//! times a second, which is as fast as a person reads a score. A connection
//! that drops is opened again after 1, 2, 4 and up to 60 seconds.

use super::fetch::{self, client};
use super::sse::{Item, Parser};
use super::state::{backoff, Plan};
use super::{bind, parse, Ctx, Feeds};
use futures_util::{SinkExt, Stream, StreamExt};
use godwinmix_protocol::feeds::MAX_BYTES;
use serde_json::Value;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig, Message};

/// The least time between two writes from one pushed feed.
const GAP: Duration = Duration::from_millis(200);
/// A websocket is pinged this often, and given up on after twice this
/// with nothing from the server.
const PING: Duration = Duration::from_secs(30);
/// An event stream with nothing, not even a comment, for this long is
/// opened again.
const SSE_IDLE: Duration = Duration::from_secs(300);

/// `Some(text)` is a message, `None` a sign of life with nothing in it.
type Messages = Pin<Box<dyn Stream<Item = Result<Option<String>, String>> + Send>>;

pub async fn websocket(feeds: Arc<Feeds>, ctx: Ctx, id: String, wake: Arc<Notify>) {
    run(feeds, ctx, id, wake, PING * 2, |plan| Box::pin(open_ws(plan))).await
}

pub async fn sse(feeds: Arc<Feeds>, ctx: Ctx, id: String, wake: Arc<Notify>) {
    run(feeds, ctx, id, wake, SSE_IDLE, |plan| Box::pin(open_sse(plan))).await
}

type Opening = Pin<Box<dyn std::future::Future<Output = Result<Messages, String>> + Send>>;

async fn run(feeds: Arc<Feeds>, ctx: Ctx, id: String, wake: Arc<Notify>, idle: Duration, open: impl Fn(Plan) -> Opening) {
    loop {
        let Some(plan) = feeds.plan(&id) else { return };
        let timeout = plan.timeout;
        let why = match tokio::time::timeout(timeout, open(plan)).await {
            Err(_) => format!("no answer within {} s; the server is slow or the address is wrong", timeout.as_secs_f64()),
            Ok(Err(e)) => e,
            Ok(Ok(messages)) => pump(&feeds, &ctx, &id, messages, idle).await,
        };
        feeds.note_failure(&ctx, &id, why);
        let wait = backoff(Duration::from_secs(1), feeds.failures(&id), Duration::from_secs(60));
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = wake.notified() => {}
        }
    }
}

/// Read until the connection ends, and say why it did.
async fn pump(feeds: &Arc<Feeds>, ctx: &Ctx, id: &str, mut messages: Messages, idle: Duration) -> String {
    let mut pending: Option<(Value, usize)> = None;
    let mut last_write = Instant::now() - GAP;
    loop {
        tokio::select! {
            next = tokio::time::timeout(idle, messages.next()) => match next {
                Err(_) => return format!("nothing from the server for {} s, so it is opened again", idle.as_secs()),
                Ok(None) => return "the server closed the connection".into(),
                Ok(Some(Err(e))) => return e,
                Ok(Some(Ok(None))) => {}
                Ok(Some(Ok(Some(text)))) => pending = Some((parse::message(&text), text.len())),
            },
            _ = tokio::time::sleep_until(last_write + GAP), if pending.is_some() => {}
        }
        if last_write.elapsed() >= GAP {
            if let Some((doc, bytes)) = pending.take() {
                let changed = feeds.note_doc(ctx, id, doc, bytes);
                bind::apply(feeds, ctx, id, !changed).await;
                last_write = Instant::now();
            }
        }
    }
}

async fn open_ws(plan: Plan) -> Result<Messages, String> {
    let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    let mut req = plan.address.as_str().into_client_request().map_err(|e| format!("the address will not do for a websocket: {e}"))?;
    for (k, v) in &plan.headers {
        let name = tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(k.as_bytes()).map_err(|e| e.to_string())?;
        req.headers_mut().insert(name, v.parse().map_err(|_| format!("the header {k} has a value HTTP cannot send"))?);
    }
    let config = WebSocketConfig::default().max_message_size(Some(MAX_BYTES)).max_frame_size(Some(MAX_BYTES));
    let (ws, _) = tokio_tungstenite::connect_async_with_config(req, Some(config), true)
        .await
        .map_err(|e| format!("the websocket would not open: {e}"))?;
    let (mut sink, stream) = ws.split();
    // A ping now and then, so a connection that died without a word is
    // noticed by the idle timeout rather than held for ever.
    let pinger = tokio::spawn(async move {
        loop {
            tokio::time::sleep(PING).await;
            if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                return;
            }
        }
    });
    let guard = AbortOnDrop(pinger);
    Ok(Box::pin(stream.map(move |m| {
        let _ = &guard;
        match m {
            Ok(Message::Text(t)) => Ok(Some(t.to_string())),
            Ok(Message::Binary(b)) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
            Ok(Message::Close(_)) => Err("the server closed the websocket".into()),
            Ok(_) => Ok(None),
            Err(e) => Err(format!("the websocket broke: {e}")),
        }
    })))
}

/// The first message a pushed feed sends, for `feed.test`.
pub async fn first(plan: Plan) -> Result<String, String> {
    let timeout = plan.timeout;
    let socket = plan.address.to_ascii_lowercase().starts_with("ws");
    let wait = async move {
        let mut messages = if socket { open_ws(plan).await? } else { open_sse(plan).await? };
        loop {
            match messages.next().await {
                Some(Ok(Some(text))) => return Ok(text),
                Some(Ok(None)) => {}
                Some(Err(e)) => return Err(e),
                None => return Err("the server closed the connection before sending anything".to_string()),
            }
        }
    };
    tokio::time::timeout(timeout, wait).await.map_err(|_| {
        format!(
            "nothing arrived within {} s. A feed that pushes only on a change may be quiet now; add it and watch feed.list",
            timeout.as_secs_f64()
        )
    })?
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn open_sse(plan: Plan) -> Result<Messages, String> {
    let mut req = client().get(&plan.address).header(reqwest::header::ACCEPT, "text/event-stream");
    for (k, v) in &plan.headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = req.send().await.map_err(|e| fetch::sentence(e, plan.timeout))?;
    if !resp.status().is_success() {
        return Err(format!("the server answered {} to the event stream; check the address and the headers", resp.status()));
    }
    let state = (resp, Parser::default(), std::collections::VecDeque::new());
    Ok(Box::pin(futures_util::stream::unfold(state, |(mut resp, mut parser, mut queue)| async move {
        loop {
            if let Some(item) = queue.pop_front() {
                let out = match item {
                    Item::Event(text) => Ok(Some(text)),
                    Item::Alive => Ok(None),
                };
                return Some((out, (resp, parser, queue)));
            }
            match resp.chunk().await {
                Ok(Some(bytes)) => match parser.push(&bytes) {
                    Ok(items) => queue.extend(items),
                    Err(e) => return Some((Err(e), (resp, parser, queue))),
                },
                Ok(None) => return None,
                Err(e) => return Some((Err(format!("the event stream broke: {}", e.without_url())), (resp, parser, queue))),
            }
        }
    })))
}
