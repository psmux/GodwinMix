//! `/hls/*` on the station. A direct show's output is found here and the
//! player let in or refused here, with the control port's own rules, and
//! then the request goes to the HLS packager on loopback, which answers it
//! from its rings. Every other show's is relayed to that show.
//!
//! The paths are a show's: `/hls/<output>/master.m3u8?show=<id>&key=...`.
//! `show` picks the show as it does on every other path, the first show
//! when it is left out, and is carried onto every URI a playlist hands out.

use crate::control::hls::{auth, refuse, Door};
use crate::station::packager::wire::{CHANNEL, PEER_HEADER, SHOW_HEADER};
use crate::station::relay::{http, show_in};
use crate::station::state::Station;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use godwinmix_core::hls::Stream;
use godwinmix_protocol::scope::Tokens;
use serde_json::json;
use std::sync::Arc;

/// The station as a door onto its direct shows' HLS outputs.
#[derive(Clone)]
pub struct StationDoor(pub(super) Arc<Station>);

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

/// The routes, for the station's router. One handler serves all four: what
/// differs between them is the packager's to answer.
pub fn router(st: Arc<Station>) -> Router {
    Router::new()
        .route("/hls/{output}/master.m3u8", get(any))
        .route("/hls/{output}/manifest.mpd", get(any))
        .route("/hls/{output}/{rung}/index.m3u8", get(any))
        .route("/hls/{output}/{rung}/{file}", get(any))
        .with_state(st)
}

async fn any(State(st): State<Arc<Station>>, req: Request) -> Response {
    let show = show_of(&st, &req);
    if !st.is_direct(&show) {
        return http::forward(st, req).await;
    }
    let output = req.uri().path().split('/').nth(2).unwrap_or_default().to_string();
    let door = StationDoor(st.clone());
    let stream = match door.find(&output, &req) {
        Ok(s) => s,
        Err(r) => return *r,
    };
    if let Err(r) = auth::admit(&door, &stream, &req) {
        return *r;
    }
    let path = req.uri().path_and_query().map(|p| p.as_str().to_string()).unwrap_or_else(|| "/".into());
    pass_on(&st, req, &show, &output, &path).await
}

/// Hand a request that was let in to the packager, at `path` on its port,
/// for the output `output` of `show`.
pub(super) async fn pass_on(st: &Arc<Station>, req: Request, show: &str, output: &str, path: &str) -> Response {
    let Some((addr, secret)) = st.direct.hls.packager() else { return not_up(st, show, output) };
    let url = format!("http://{addr}{path}");
    let (mut parts, body) = req.into_parts();
    // The player was let in here; the packager takes the station's word.
    parts.headers.remove(header::AUTHORIZATION);
    if let Ok(v) = HeaderValue::from_str(&format!("Bearer {secret}")) {
        parts.headers.insert(header::AUTHORIZATION, v);
    }
    if let Ok(v) = HeaderValue::from_str(show) {
        parts.headers.insert(SHOW_HEADER, v);
    }
    parts.headers.remove(PEER_HEADER);
    let peer = parts.extensions.get::<ConnectInfo<std::net::SocketAddr>>().map(|c| c.0.to_string());
    if let Some(v) = peer.and_then(|p| HeaderValue::from_str(&p).ok()) {
        parts.headers.insert(PEER_HEADER, v);
    }
    match http::pass(&st.http, &url, Request::from_parts(parts, body)).await {
        Ok(answer) => answer,
        Err(_) => not_up(st, show, output),
    }
}

/// A 503 a player retries, while the packager is starting or starting again.
fn not_up(st: &Station, show: &str, output: &str) -> Response {
    let why = st.direct.hls.view(show, output).and_then(|(l, _)| l.error).unwrap_or_else(|| "it is starting".into());
    let whose = match show.strip_prefix(CHANNEL) {
        Some(channel) => format!("Channel {channel}'s watch link"),
        None => format!("Show {show}'s HLS packager"),
    };
    let message = format!("{whose} is not answering now: {why}. Try again in a few seconds; a player does by itself.");
    let mut r = refuse(StatusCode::SERVICE_UNAVAILABLE, message, json!({ "show": show, "output": output, "retry_after_s": 2 }));
    r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("2"));
    r
}
