//! `/streamer/api/v3`: the streams in Flussonic's JSON, filled from the
//! shows, and edits to the config through the lossless writer. Basic auth
//! with the file's `edit_auth`, or `view_auth` for reading.

use super::state::Headend;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use godwinmix_flussonic::api::auth::{check, Access, Refusal};
use godwinmix_flussonic::api::{self as shape, edit, sanitised, Live};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;

type St = State<Arc<Headend>>;

pub fn router() -> Router<Arc<Headend>> {
    Router::new()
        .route("/streamer/api/v3/streams", get(list))
        .route("/streamer/api/v3/streams/{name}", get(one).put(put).delete(delete))
        .route("/streamer/api/v3/config", get(config))
        .route("/streamer/api/v3/health", get(health))
        .route("/health", get(health))
}

fn refuse(status: StatusCode, msg: impl Into<String>) -> Response {
    let mut r = (status, Json(json!({"error": {"message": msg.into()}}))).into_response();
    if status == StatusCode::UNAUTHORIZED {
        r.headers_mut().insert(header::WWW_AUTHENTICATE, header::HeaderValue::from_static("Basic realm=\"Flussonic\""));
    }
    r
}

fn allowed(h: &Headend, headers: &HeaderMap, peer: SocketAddr, want: Access) -> Result<(), Box<Response>> {
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    let refused = match check(&h.doc.lock().config, auth, want, peer.ip().is_loopback()) {
        Ok(()) => return Ok(()),
        Err(Refusal::Unauthorized) => refuse(StatusCode::UNAUTHORIZED, "send the edit_auth (or view_auth) user and password of the config as Basic auth"),
        Err(Refusal::ReadOnly) => refuse(StatusCode::FORBIDDEN, "view_auth reads; changes need edit_auth"),
        Err(Refusal::NoAuthConfigured) => refuse(StatusCode::FORBIDDEN, "the config has no edit_auth, so the API answers this machine only. Add edit_auth to open it to others."),
    };
    Err(Box::new(refused))
}

fn answered(h: &Headend, name: Option<&str>) -> Vec<(godwinmix_flussonic::model::Stream, Live)> {
    let live = h.live.read();
    let doc = h.doc.lock();
    doc.config.streams.iter().filter(|s| name.is_none_or(|n| s.name == n)).map(|s| (s.clone(), live.get(&s.name).cloned().unwrap_or_default())).collect()
}

async fn list(State(h): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Response {
    if let Err(r) = allowed(&h, &headers, peer, Access::Read) {
        return *r;
    }
    Json(shape::list(&answered(&h, None))).into_response()
}

async fn one(State(h): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    if let Err(r) = allowed(&h, &headers, peer, Access::Read) {
        return *r;
    }
    match answered(&h, Some(&name)).first() {
        Some((s, l)) => Json(shape::stream(s, l)).into_response(),
        None => refuse(StatusCode::NOT_FOUND, format!("no stream {name}")),
    }
}

/// Write the document back to the file, atomically, and apply it.
async fn save(h: &Arc<Headend>) -> Result<(), Box<Response>> {
    let text = h.doc.lock().text();
    let tmp = h.file.with_extension("conf.godwinmix-tmp");
    std::fs::write(&tmp, &text)
        .and_then(|_| std::fs::rename(&tmp, &h.file))
        .map_err(|e| Box::new(refuse(StatusCode::INTERNAL_SERVER_ERROR, format!("could not write {}: {e}", h.file.display()))))?;
    super::apply::apply_file(h, false).await;
    h.changed.notify_one();
    Ok(())
}

async fn put(State(h): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Path(name): Path<String>, body: axum::body::Bytes) -> Response {
    if let Err(r) = allowed(&h, &headers, peer, Access::Edit) {
        return *r;
    }
    let Ok(v) = serde_json::from_slice::<Value>(&body) else { return refuse(StatusCode::BAD_REQUEST, "the body is not JSON") };
    let edited = {
        let mut doc = h.doc.lock();
        let mut next = doc.clone();
        edit::put(&mut next, &name, &v).map(|_| *doc = next)
    };
    if let Err(e) = edited {
        return refuse(StatusCode::BAD_REQUEST, e.to_string());
    }
    if let Err(r) = save(&h).await {
        return *r;
    }
    one(State(h), ConnectInfo(peer), headers, Path(name)).await
}

async fn delete(State(h): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    if let Err(r) = allowed(&h, &headers, peer, Access::Edit) {
        return *r;
    }
    if let Err(e) = h.doc.lock().remove_stream(&name) {
        return refuse(StatusCode::NOT_FOUND, e.to_string());
    }
    match save(&h).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(r) => *r,
    }
}

async fn config(State(h): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Response {
    if let Err(r) = allowed(&h, &headers, peer, Access::Read) {
        return *r;
    }
    Json(sanitised::config(&h.doc.lock().config)).into_response()
}

/// No auth: for a probe. Counts only.
async fn health(State(h): St) -> Response {
    let live = h.live.read();
    let streams = h.doc.lock().config.streams.len();
    let running = live.values().filter(|l| l.input_live && l.output_live).count();
    Json(json!({"status": "ok", "server": "godwinmix", "streams": streams, "shows": h.running.lock().len(), "running": running})).into_response()
}
