//! `/hls/*`: HLS and LL-HLS from the control port, no new port.
//!
//! | Route | Answer |
//! |---|---|
//! | `GET /hls/{output}/master.m3u8` | the multivariant playlist |
//! | `GET /hls/{output}/{rung}/index.m3u8` | a media playlist; `_HLS_msn` and `_HLS_part` block |
//! | `GET /hls/{output}/{rung}/init.mp4` | the init segment (`init1.mp4` after a rebuild) |
//! | `GET /hls/{output}/{rung}/{n}.m4s` | a whole segment |
//! | `GET /hls/{output}/{rung}/{n}.{p}.m4s` | one LL-HLS part |
//!
//! # Who may read
//!
//! Either the control token with the `read` scope, the same rule as every
//! other preview door (`Authorization: Bearer`, or `?token=` on a GET), or
//! the output's own viewer key as `?key=`. The viewer key opens this one
//! output's playlists and segments and nothing else on the port, so it is
//! what a link for viewers carries; the page never puts the control token in
//! a link. Whatever came in the query is written onto every URI a playlist
//! hands out, so a player that cannot send a header keeps presenting it.
//!
//! # Waiting
//!
//! A blocking playlist reload, and a request for the part the playlist
//! hinted, wait on the rung's `watch` channel with a deadline. The wait is a
//! future on the server's runtime; it holds no lock and no thread. The
//! packager never waits for anybody.

mod auth;
mod files;
mod playlists;

use crate::control::Ctx;
use axum::body::{Body, Bytes};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde_json::{json, Value};

pub fn router(ctx: Ctx) -> Router<Ctx> {
    Router::new()
        .route("/hls/{output}/master.m3u8", get(playlists::master))
        .route("/hls/{output}/{rung}/index.m3u8", get(playlists::media))
        .route("/hls/{output}/{rung}/{file}", get(files::file))
        .with_state(ctx)
}

const PLAYLIST: &str = "application/vnd.apple.mpegurl";

/// A refusal a player logs and a person can read: a status, and a JSON body
/// with the message and what to do next.
fn refuse(code: StatusCode, message: String, data: Value) -> Response {
    let mut r = (code, axum::Json(json!({ "error": message, "data": data }))).into_response();
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

fn playlist_response(text: String) -> Response {
    let mut r = (StatusCode::OK, text).into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(PLAYLIST));
    // A playlist changes every part; nothing between here and the player
    // may keep one.
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

/// Bytes the ring already holds, handed out by reference: the body is the
/// ring's own `Bytes`, one per part, and nothing is copied per viewer.
fn media_response(parts: Vec<Bytes>, content_type: &'static str, max_age: u32) -> Response {
    let len: usize = parts.iter().map(Bytes::len).sum();
    let stream = futures_util::stream::iter(parts.into_iter().map(Ok::<_, std::io::Error>));
    let mut r = Body::from_stream(stream).into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    // A segment never changes once it is whole, so it may be kept for as long
    // as it could still be asked for.
    if let Ok(v) = HeaderValue::from_str(&format!("max-age={max_age}, immutable")) {
        h.insert(header::CACHE_CONTROL, v);
    }
    r
}
