//! `/whip/<channel>/<stream>`: WHIP ingest for a channel, on the control port.
//!
//! A browser, OBS or any WHIP client publishes to a channel here, on the port
//! the page already uses, so WebRTC ingest opens no TCP port of its own. The
//! key is the bearer token (`Authorization: Bearer <key>`), or `?psk=` for a
//! client that cannot set a header. This is not behind the control token:
//! the channel's key is what lets a publisher in, as it is for RTMP and SRT.
//!
//! | Route | Answer |
//! |---|---|
//! | `POST /whip/{channel}/{stream}` | `201` with the SDP answer and the session's `Location` |
//! | `DELETE /whip/{channel}/{stream}/{session}` | `200`, the session ends |
//! | `PATCH /whip/{channel}/{stream}/{session}` | `405`: candidates are all in the answer, no trickle |
//!
//! The work is the ingest plugin's (`plugins/ingest/src/whip.rs`); this is
//! the HTTP and nothing else.

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;

use crate::channels::{Channels, Whip};
use crate::control::Ctx;
use std::sync::Arc;

pub fn router(ctx: Ctx) -> Router<Ctx> {
    router_for(ctx.app.channels.clone())
}

/// The same routes over any channel registry: a single process core's, or a
/// station's, which serves WHIP for every show.
pub fn router_for<S: Clone + Send + Sync + 'static>(channels: Arc<Channels>) -> Router<S> {
    Router::new()
        .route("/whip/{channel}/{stream}", post(offer))
        .route("/whip/{channel}/{stream}/{session}", axum::routing::delete(end).patch(no_trickle))
        .with_state(channels)
}

/// The key: a bearer token, or `?psk=`, `?key=` or `?token=`.
fn key_of(headers: &HeaderMap, query: &HashMap<String, String>) -> String {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(|v| v.trim().to_string());
    bearer
        .or_else(|| ["psk", "key", "token"].iter().find_map(|k| query.get(*k).cloned()))
        .unwrap_or_default()
}

async fn offer(
    State(channels): State<Arc<Channels>>,
    Path((channel, stream)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
    extensions: axum::http::Extensions,
    headers: HeaderMap,
    body: String,
) -> Response {
    let sdp_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !sdp_type.starts_with("application/sdp") {
        let why = "a WHIP offer is the SDP itself, sent with Content-Type: application/sdp.";
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, why).into_response();
    }
    let key = key_of(&headers, &query);
    let peer = extensions.get::<ConnectInfo<SocketAddr>>().map(|p| p.0.to_string()).unwrap_or_else(|| "unknown".into());
    let (app, name) = (channel.clone(), stream.clone());
    let answer = tokio::task::spawn_blocking(move || channels.whip_offer(&app, &name, &key, &body, &peer)).await;
    match answer {
        Ok(Whip::Answer { session, sdp }) => {
            let location = format!("/whip/{channel}/{stream}/{session}");
            (StatusCode::CREATED, [(header::CONTENT_TYPE, "application/sdp".to_string()), (header::LOCATION, location)], sdp).into_response()
        }
        Ok(Whip::Refused { status, why }) => {
            (StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST), why).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("the WHIP offer was not handled: {e}")).into_response(),
    }
}

async fn end(State(channels): State<Arc<Channels>>, Path((_, _, session)): Path<(String, String, String)>) -> Response {
    match tokio::task::spawn_blocking(move || channels.whip_end(&session)).await {
        Ok(true) => StatusCode::OK.into_response(),
        _ => (StatusCode::NOT_FOUND, "no WHIP session by that name; it may have ended already.").into_response(),
    }
}

async fn no_trickle() -> Response {
    let why = "this WHIP endpoint puts every candidate in its answer and takes none later. Send the offer once gathering is complete.";
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "DELETE")], why).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_the_bearer_token_or_a_query_parameter() {
        let mut headers = HeaderMap::new();
        let none = HashMap::new();
        assert_eq!(key_of(&headers, &none), "");
        let query: HashMap<String, String> = [("psk".to_string(), "from-query".to_string())].into();
        assert_eq!(key_of(&headers, &query), "from-query");
        headers.insert(header::AUTHORIZATION, "Bearer from-header".parse().unwrap());
        assert_eq!(key_of(&headers, &query), "from-header", "the header wins");
    }
}
