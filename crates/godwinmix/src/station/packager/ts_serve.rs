//! The TS listings as data, for the station: what its Flussonic facade
//! names files from (`station::direct::hls::facade`). Behind the
//! packager's secret, like everything on this port.
//!
//! | Request | Answer |
//! |---|---|
//! | `GET /packager/ts/master?output=` | `ts::Master` as JSON |
//! | `GET /packager/ts/media?output=&rendition=` | `ts::Media` as JSON |
//! | `GET /packager/ts/segment?output=&rendition=&at=` | the segment, `at` a `ts::At` as JSON |

use super::serve::PackagerDoor;
use super::wire::SHOW_HEADER;
use crate::control::hls::refuse;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use godwinmix_core::hls::ts::At;
use godwinmix_core::hls::Stream;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub const MASTER: &str = "/packager/ts/master";
pub const MEDIA: &str = "/packager/ts/media";
pub const SEGMENT: &str = "/packager/ts/segment";

#[derive(Deserialize)]
pub struct Ask {
    output: String,
    #[serde(default)]
    rendition: String,
    #[serde(default)]
    at: Option<String>,
}

pub fn router(door: PackagerDoor) -> Router {
    Router::new().route(MASTER, get(master)).route(MEDIA, get(media)).route(SEGMENT, get(segment)).with_state(door)
}

fn find(door: &PackagerDoor, headers: &HeaderMap, ask: &Ask) -> Result<Arc<Stream>, Box<Response>> {
    if !door.station(headers) {
        return Err(Box::new(super::serve::not_the_station()));
    }
    let show = headers.get(SHOW_HEADER).and_then(|v| v.to_str().ok()).unwrap_or_default();
    door.outputs.stream(show, &ask.output).ok_or_else(|| {
        Box::new(refuse(StatusCode::NOT_FOUND, format!("show {show} has no HLS output `{}` in the packager now", ask.output), json!({ "show": show })))
    })
}

fn not_yet(what: &str) -> Response {
    refuse(StatusCode::SERVICE_UNAVAILABLE, format!("{what} is not made yet; try again in a few seconds"), json!({ "retry_after_s": 2 }))
}

async fn master(State(door): State<PackagerDoor>, headers: HeaderMap, Query(ask): Query<Ask>) -> Response {
    match find(&door, &headers, &ask) {
        Ok(s) => s.ts_master().map_or_else(|| not_yet("The first MPEG-TS segment"), |m| Json(m).into_response()),
        Err(r) => *r,
    }
}

async fn media(State(door): State<PackagerDoor>, headers: HeaderMap, Query(ask): Query<Ask>) -> Response {
    match find(&door, &headers, &ask) {
        Ok(s) => s.ts_media(&ask.rendition).map_or_else(|| not_yet(&format!("MPEG-TS rendition `{}`", ask.rendition)), |m| Json(m).into_response()),
        Err(r) => *r,
    }
}

async fn segment(State(door): State<PackagerDoor>, headers: HeaderMap, Query(ask): Query<Ask>) -> Response {
    let stream = match find(&door, &headers, &ask) {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let Some(at) = ask.at.as_deref().and_then(|a| serde_json::from_str::<At>(a).ok()) else {
        return refuse(StatusCode::BAD_REQUEST, "at must be a ts::At as JSON".into(), json!({}));
    };
    match stream.ts_segment(&ask.rendition, at) {
        Some(bytes) => crate::control::hls::media_response(vec![bytes], "video/mp2t", stream.params.window_s * 2),
        None => refuse(StatusCode::NOT_FOUND, format!("no such segment of `{}` in the window", ask.rendition), json!({ "at": ask.at })),
    }
}
