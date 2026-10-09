//! `/hls/channel/<channel>/<destination>/...` on the station: a channel's
//! watch link. The player is let in here with the link's viewer key, as a
//! show's output lets one in, and the request goes on to the packager under
//! the path a show's output has there.
//!
//! | Route | Goes to the packager as |
//! |---|---|
//! | `.../index.m3u8` | `/hls/<destination>/master.m3u8` |
//! | `.../manifest.mpd` | `/hls/<destination>/manifest.mpd` |
//! | `.../<rung>/index.m3u8` | `/hls/<destination>/<rung>/index.m3u8` |
//! | `.../<rung>/<file>` | `/hls/<destination>/<rung>/<file>` |

use super::channel::key;
use super::serve::{pass_on, StationDoor};
use crate::control::hls::{auth, refuse};
use crate::station::state::Station;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde_json::json;
use std::sync::Arc;

pub fn router(st: Arc<Station>) -> Router {
    Router::new()
        .route("/hls/channel/{channel}/{destination}/index.m3u8", get(top))
        .route("/hls/channel/{channel}/{destination}/manifest.mpd", get(top))
        .route("/hls/channel/{channel}/{destination}/{rung}/index.m3u8", get(rung))
        .route("/hls/channel/{channel}/{destination}/{rung}/{file}", get(rung))
        .with_state(st)
}

async fn top(State(st): State<Arc<Station>>, Path((channel, destination)): Path<(String, String)>, req: Request) -> Response {
    let file = if req.uri().path().ends_with(".mpd") { "manifest.mpd" } else { "master.m3u8" };
    answer(st, &channel, &destination, file, req).await
}

async fn rung(State(st): State<Arc<Station>>, Path(parts): Path<Vec<(String, String)>>, req: Request) -> Response {
    let get = |k: &str| parts.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap_or_default();
    let (channel, destination, rung) = (get("channel"), get("destination"), get("rung"));
    let file = req.uri().path().rsplit('/').next().unwrap_or_default().to_string();
    answer(st, &channel, &destination, &format!("{rung}/{file}"), req).await
}

async fn answer(st: Arc<Station>, channel: &str, destination: &str, file: &str, req: Request) -> Response {
    let (show, output) = key(channel, destination);
    let Some(stream) = st.direct.hls.stream(&show, &output) else { return missing(&st, channel, destination) };
    if let Err(r) = auth::admit(&StationDoor(st.clone()), &stream, &req) {
        return *r;
    }
    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    let path = format!("/hls/{output}/{file}{query}");
    pass_on(&st, req, &show, &output, &path).await
}

/// A 404 that says which links the channel has, or how to make one.
fn missing(st: &Station, channel: &str, destination: &str) -> Response {
    let (show, _) = key(channel, destination);
    let have = st.direct.hls.ids(&show);
    let message = if have.is_empty() {
        format!(
            "Channel {channel} has no watch link switched on. Add one with channel.destination.add \
             {{id: \"{channel}\", platform: \"hls\"}}, or switch its link on with channel.destination.set."
        )
    } else {
        format!("Channel {channel} has no watch link `{destination}`. It has {}.", have.join(", "))
    };
    refuse(StatusCode::NOT_FOUND, message, json!({ "channel": channel, "destination": destination, "links": have }))
}
