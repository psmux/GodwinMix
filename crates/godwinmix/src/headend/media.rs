//! `/<name>/...`: playlists, segments, the picture, the player page and live
//! MPEG-TS, each named the way Flussonic names it.

use super::facade::say;
use super::state::Headend;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use godwinmix_flussonic::facade::access::{At, Container, Missing, Select};
use godwinmix_flussonic::facade::paths::{route, select_text, Route};
use godwinmix_flussonic::facade::{embed, playlist};
use std::sync::Arc;
use std::time::Duration;

const PLAYLIST: &str = "application/vnd.apple.mpegurl";

fn missing(m: Missing) -> Response {
    match m {
        Missing::Gone(w) => say(StatusCode::NOT_FOUND, w),
        Missing::NotYet(w) => say(StatusCode::SERVICE_UNAVAILABLE, w),
        Missing::Unsupported(w) => say(StatusCode::NOT_IMPLEMENTED, w),
    }
}

fn body(kind: &'static str, cache: &'static str, bytes: impl Into<Body>) -> Response {
    ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, cache)], bytes.into()).into_response()
}

pub async fn serve(State(h): State<Arc<Headend>>, Path((name, rest)): Path<(String, String)>) -> Response {
    let Some(r) = route(&rest) else { return say(StatusCode::NOT_FOUND, format!("{name} has no {rest}")) };
    let Some(at) = h.at(&name) else {
        let known = h.doc.lock().config.streams.iter().any(|s| s.name == name);
        let why = if known { format!("stream {name} is in the config and not running: it is disabled, or could not be made (the log says why)") } else { format!("no stream {name}") };
        return say(StatusCode::NOT_FOUND, why);
    };
    match answer(&h, &name, &at, r).await {
        Ok(r) => r,
        Err(m) => missing(m),
    }
}

async fn default_select(h: &Headend, at: &At) -> Result<Select, Missing> {
    Ok(playlist::default_select(&h.segments.tracks(at).await?))
}

async fn answer(h: &Arc<Headend>, name: &str, at: &At, r: Route) -> Result<Response, Missing> {
    let s = &h.segments;
    Ok(match r {
        Route::Master => body(PLAYLIST, "no-cache", playlist::master(&s.tracks(at).await?)),
        Route::Media(None) => {
            let sel = default_select(h, at).await?;
            body(PLAYLIST, "no-cache", playlist::media(&s.window(at, sel).await?, &format!("tracks-{}/", select_text(sel))))
        }
        Route::Media(Some(sel)) => body(PLAYLIST, "no-cache", playlist::media(&s.window(at, sel).await?, "")),
        Route::Segment { sel, start_s, ext } => {
            let kind = if ext == "ts" { "video/mp2t" } else { "video/iso.segment" };
            body(kind, "public, max-age=600, immutable", s.segment(at, sel, start_s).await?)
        }
        Route::Init { sel, id } => body("video/mp4", "public, max-age=600, immutable", s.init(at, sel, &id).await?),
        Route::Preview => match super::picture::jpeg(h, &at.show).await {
            Ok(jpeg) => body("image/jpeg", "no-cache", jpeg),
            Err(why) => say(StatusCode::SERVICE_UNAVAILABLE, why),
        },
        Route::Embed => {
            let title = h.doc.lock().config.streams.iter().find(|s| s.name == name).and_then(|s| s.title.clone()).unwrap_or_else(|| name.to_string());
            body("text/html; charset=utf-8", "no-cache", embed::page(&title))
        }
        Route::MpegTs => mpegts(h.clone(), at.clone()).await?,
    })
}

/// Live MPEG-TS over HTTP: the muxed segments one after another, as they
/// are made, for as long as the client reads.
async fn mpegts(h: Arc<Headend>, at: At) -> Result<Response, Missing> {
    let tracks = h.segments.tracks(&at).await?;
    let sel = playlist::default_select(&tracks);
    let w = h.segments.window(&at, sel).await?;
    if !tracks.muxed || w.container != Container::Ts {
        return Err(Missing::Unsupported("live MPEG-TS needs the outputs' TS segments, which this build does not make yet".into()));
    }
    let after = w.segments.iter().rev().nth(1).map(|s| s.start_ms).unwrap_or(i64::MIN);
    let stream = futures_util::stream::unfold((h, at, after), move |(h, at, mut after)| async move {
        loop {
            let w = h.segments.window(&at, sel).await.ok()?;
            if let Some(next) = w.segments.iter().find(|s| s.start_ms > after) {
                after = next.start_ms;
                let bytes = h.segments.segment(&at, sel, next.start_ms.div_euclid(1000)).await.ok()?;
                return Some((Ok::<_, std::io::Error>(bytes), (h, at, after)));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
    Ok(body("video/mp2t", "no-cache", Body::from_stream(stream)))
}
