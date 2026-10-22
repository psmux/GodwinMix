//! The port Flussonic answered on: its playlists and segments, its player
//! files, and `/streamer/api/v3`. The playback paths need no key, as
//! Flussonic's did not: they are for a gateway on the same machine that
//! checks its own signature, and the port binds as the `http` line says.

use super::state::Headend;
use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use godwinmix_flussonic::facade::embed;
use godwinmix_flussonic::model::Config;
use std::sync::Arc;

/// The addresses to listen on: every `http` line, or one with `port` and
/// `address` given.
pub fn listeners(cfg: &Config, port: Option<u16>, address: Option<&str>) -> Vec<String> {
    let host = |a: Option<&str>| match a {
        Some(a) if a.contains(':') => format!("[{a}]"),
        Some(a) => a.to_string(),
        None => "0.0.0.0".to_string(),
    };
    if port.is_some() || address.is_some() {
        let first = cfg.listeners.first();
        let p = port.or(first.map(|l| l.port)).unwrap_or(80);
        return vec![format!("{}:{p}", host(address.or(first.and_then(|l| l.addr.as_deref()))))];
    }
    cfg.listeners.iter().map(|l| format!("{}:{}", host(l.addr.as_deref()), l.port)).collect()
}

pub fn router(h: Arc<Headend>) -> Router {
    Router::new()
        .merge(super::api::router())
        .route("/flu/player/{file}", get(player))
        .route("/{name}/{*rest}", get(super::media::serve))
        .with_state(h)
}

async fn player(Path(file): Path<String>) -> Response {
    match embed::player_file(&file) {
        Some((bytes, kind)) => ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "public, max-age=86400")], bytes).into_response(),
        None => (StatusCode::NOT_FOUND, "no such player file").into_response(),
    }
}

/// A plain answer with a sentence, as Flussonic's own errors are.
pub fn say(status: StatusCode, why: impl Into<String>) -> Response {
    let mut r = (status, format!("{}\n", why.into())).into_response();
    r.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-cache"));
    if status == StatusCode::SERVICE_UNAVAILABLE {
        r.headers_mut().insert(header::RETRY_AFTER, header::HeaderValue::from_static("2"));
    }
    r
}
