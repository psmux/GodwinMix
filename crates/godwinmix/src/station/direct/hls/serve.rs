//! `/hls/*` on the station: a direct show's outputs answered here, with the
//! control port's own handlers, and every other show's relayed to it.
//!
//! The paths are a show's: `/hls/<output>/master.m3u8?show=<id>&key=...`.
//! `show` picks the show as it does on every other path, the first show
//! when it is left out, and is carried onto every URI a playlist hands out.

use crate::control::hls::{files, playlists, refuse, Door};
use crate::station::relay::{http, show_in};
use crate::station::state::Station;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use godwinmix_core::hls::Stream;
use godwinmix_protocol::scope::Tokens;
use serde_json::json;
use std::sync::Arc;

/// The station as a door onto its direct shows' HLS outputs.
#[derive(Clone)]
pub struct StationDoor(Arc<Station>);

fn show_of(st: &Station, req: &Request) -> String {
    show_in(req.uri().query()).unwrap_or_else(|| st.first())
}

impl Door for StationDoor {
    fn tokens(&self) -> &Tokens {
        &self.0.tokens
    }

    fn find(&self, output: &str, req: &Request) -> Result<Arc<Stream>, Box<Response>> {
        let show = show_of(&self.0, req);
        let hls = &self.0.direct.hls;
        hls.stream(&show, output).ok_or_else(|| {
            let have = hls.ids(&show);
            let message = if have.is_empty() {
                format!(
                    "Show {show} serves no HLS now. Add one with show.output.add {{id: \"{show}\", uri: \"hls://{output}\"}}; \
                     an output that is switched off, or a show that is stopped, serves nothing."
                )
            } else {
                format!("Show {show} has no HLS output `{output}`. It serves {}.", have.join(", "))
            };
            Box::new(refuse(StatusCode::NOT_FOUND, message, json!({ "show": show, "output": output, "outputs": have })))
        })
    }
}

/// The routes, for the station's router.
pub fn router(st: Arc<Station>) -> Router {
    Router::new()
        .route("/hls/{output}/master.m3u8", get(master))
        .route("/hls/{output}/manifest.mpd", get(dash))
        .route("/hls/{output}/{rung}/index.m3u8", get(media))
        .route("/hls/{output}/{rung}/{file}", get(file))
        .with_state(st)
}

/// Whether this request is for a direct show, which the station answers.
fn here(st: &Station, req: &Request) -> bool {
    st.is_direct(&show_of(st, req))
}

async fn master(State(st): State<Arc<Station>>, path: Path<String>, req: Request) -> Response {
    if !here(&st, &req) {
        return http::forward(st, req).await;
    }
    playlists::master(State(StationDoor(st)), path, req).await
}

async fn dash(State(st): State<Arc<Station>>, path: Path<String>, req: Request) -> Response {
    if !here(&st, &req) {
        return http::forward(st, req).await;
    }
    playlists::dash(State(StationDoor(st)), path, req).await
}

async fn media(State(st): State<Arc<Station>>, path: Path<(String, String)>, req: Request) -> Response {
    if !here(&st, &req) {
        return http::forward(st, req).await;
    }
    playlists::media(State(StationDoor(st)), path, req).await
}

async fn file(State(st): State<Arc<Station>>, path: Path<(String, String, String)>, req: Request) -> Response {
    if !here(&st, &req) {
        return http::forward(st, req).await;
    }
    files::file(State(StationDoor(st)), path, req).await
}
