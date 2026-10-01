//! `/hls/*`: HLS and LL-HLS from the control port, no new port.
//!
//! | Route | Answer |
//! |---|---|
//! | `GET /hls/{output}/master.m3u8` | the multivariant playlist |
//! | `GET /hls/{output}/manifest.mpd` | the same segments as a DASH MPD |
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
//! # Who answers
//!
//! The routes are the same wherever they are served, and a [`Door`] says
//! where the outputs are and whose tokens let a request in. A core's door is
//! its [`Ctx`]: the engine's own registry of `hls/output`s. A station has
//! two for the HLS outputs of shows without compositing, served under the
//! same paths with `?show=<id>`: `station::direct::hls` finds the output
//! and lets the player in, and the HLS packager process
//! (`station::packager`) answers from its rings.
//!
//! # Waiting
//!
//! A blocking playlist reload, and a request for the part the playlist
//! hinted, wait on the rung's `watch` channel with a deadline. The wait is a
//! future on the server's runtime; it holds no lock and no thread. The
//! packager never waits for anybody.

pub mod auth;
pub mod files;
pub mod playlists;

use crate::control::Ctx;
use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use godwinmix_core::hls::{stream, Stream};
use godwinmix_protocol::scope::Tokens;
use serde_json::{json, Value};
use std::sync::Arc;

/// Where `/hls/*` finds an output, and whose tokens open it.
pub trait Door: Clone + Send + Sync + 'static {
    fn tokens(&self) -> &Tokens;
    /// The output `req` names, or a 404 that names the ones there are.
    fn find(&self, output: &str, req: &Request) -> Result<Arc<Stream>, Box<Response>>;
}

impl Door for Ctx {
    fn tokens(&self) -> &Tokens {
        &self.app.tokens
    }

    fn find(&self, id: &str, _: &Request) -> Result<Arc<Stream>, Box<Response>> {
        stream::get(id).ok_or_else(|| {
            let have = stream::ids();
            let message = if have.is_empty() {
                format!("There is no HLS output `{id}`, and none running. Add one with output.add {{type: \"hls/output\"}}.")
            } else {
                format!("There is no HLS output `{id}`. This core serves {}.", have.join(", "))
            };
            Box::new(refuse(StatusCode::NOT_FOUND, message, json!({ "output": id, "outputs": have })))
        })
    }
}

/// The four routes, answered through `door`.
pub fn router<D: Door>(door: D) -> Router<D> {
    Router::new()
        .route("/hls/{output}/master.m3u8", get(playlists::master::<D>))
        .route("/hls/{output}/manifest.mpd", get(playlists::dash::<D>))
        .route("/hls/{output}/{rung}/index.m3u8", get(playlists::media::<D>))
        .route("/hls/{output}/{rung}/{file}", get(files::file::<D>))
        .with_state(door)
}

const PLAYLIST: &str = "application/vnd.apple.mpegurl";

/// A refusal a player logs and a person can read: a status, and a JSON body
/// with the message and what to do next.
pub fn refuse(code: StatusCode, message: String, data: Value) -> Response {
    let mut r = (code, axum::Json(json!({ "error": message, "data": data }))).into_response();
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

fn playlist_response(text: String) -> Response {
    listing(text, PLAYLIST)
}

/// A playlist or an MPD: text that changes every part and may not be kept.
fn listing(text: String, content_type: &'static str) -> Response {
    let mut r = (StatusCode::OK, text).into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
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
