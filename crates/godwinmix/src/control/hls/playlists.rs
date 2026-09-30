//! The two playlist routes.

use super::auth::{self, Viewer};
use super::{playlist_response, refuse};
use crate::control::Ctx;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use godwinmix_core::hls::playlist::with_query;
use godwinmix_core::hls::ring::Position;
use godwinmix_core::hls::track::Track;
use godwinmix_core::hls::{stream, Stream};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The output under `id`, or a 404 that names the ones there are.
pub fn find(id: &str) -> Result<Arc<Stream>, Box<Response>> {
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

fn open(ctx: &Ctx, id: &str, req: &Request) -> Result<(Arc<Stream>, Viewer), Box<Response>> {
    let stream = find(id)?;
    let viewer = auth::admit(ctx, &stream, req)?;
    Ok((stream, viewer))
}

/// A 503 a player retries, for an output with nothing to say yet.
fn not_yet(stream: &Stream, what: &str) -> Response {
    let mut r = refuse(
        StatusCode::SERVICE_UNAVAILABLE,
        format!("{what} is not made yet. {} starts a few seconds after the programme reaches it; try again.", stream.id),
        json!({ "output": stream.id, "retry_after_s": 2 }),
    );
    r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("2"));
    r
}

pub async fn master(State(ctx): State<Ctx>, Path(output): Path<String>, req: Request) -> Response {
    let (stream, mut viewer) = match open(&ctx, &output, &req) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    viewer.ensure_id();
    // Asked for before every rung has a segment: wait a little, since the
    // bandwidth and the codecs are what the playlist is for.
    let limit = Duration::from_millis(u64::from(stream.params.segment_ms) * 3).max(Duration::from_secs(6));
    let deadline = Instant::now() + limit;
    while !stream.ready() {
        if Instant::now() >= deadline {
            return not_yet(&stream, "The first segment of every rung");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    playlist_response(with_query(&stream.master(), &viewer.query))
}

pub async fn media(State(ctx): State<Ctx>, Path((output, rung)): Path<(String, String)>, req: Request) -> Response {
    let (stream, viewer) = match open(&ctx, &output, &req) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let Some(track) = stream.track(&rung) else {
        let rungs: Vec<String> = stream.tracks().iter().map(|t| t.id.clone()).collect();
        let message = format!("{output} has no rung `{rung}`. It has {}.", rungs.join(", "));
        return refuse(StatusCode::NOT_FOUND, message, json!({ "output": output, "rungs": rungs }));
    };
    let pairs = auth::pairs(&req);
    let msn = auth::value(&pairs, "_HLS_msn").map(str::parse::<u64>);
    let part = auth::value(&pairs, "_HLS_part").map(str::parse::<u32>);
    let target = Duration::from_millis(u64::from(stream.params.segment_ms).div_ceil(1000) * 1000);
    let waited = match (msn, part) {
        (None, None) => Ok(wait(&track, target * 3, |p| p.complete.is_some() || p.open.is_some_and(|(_, n)| n > 0)).await),
        (Some(Ok(m)), None) => block(&track, m, None, target).await,
        (Some(Ok(m)), Some(Ok(p))) => block(&track, m, Some(p), target).await,
        _ => {
            let message = "_HLS_msn must be a segment number, and _HLS_part a part number that comes with one".to_string();
            return refuse(StatusCode::BAD_REQUEST, message, json!({ "rung": rung }));
        }
    };
    match waited {
        Ok(true) => {}
        Ok(false) => return not_yet(&stream, &format!("The playlist {rung} was asked to wait for")),
        Err(r) => return *r,
    }
    let text = track.playlist(&stream.reports_for(&rung));
    playlist_response(with_query(std::str::from_utf8(&text).unwrap_or(""), &viewer.query))
}

/// An LL-HLS blocking reload: hold until the playlist carries segment `msn`
/// (part `part` of it, when named), for at most three target durations.
async fn block(track: &Track, msn: u64, part: Option<u32>, target: Duration) -> Result<bool, Box<Response>> {
    let newest = track.position().newest().unwrap_or(0);
    if msn > newest + 2 {
        let message = format!(
            "_HLS_msn={msn} is more than two segments past the newest, {newest}. Reload without it to catch up."
        );
        return Err(Box::new(refuse(StatusCode::BAD_REQUEST, message, json!({ "newest": newest }))));
    }
    Ok(wait(track, target * 3, |p| p.reached(msn, part)).await)
}

/// Wait on the rung's position until `done`, or `limit`. A future on the
/// server's runtime: no thread, and no lock held while it waits.
pub async fn wait(track: &Track, limit: Duration, mut done: impl FnMut(&Position) -> bool) -> bool {
    let mut rx = track.watch();
    let reached = matches!(tokio::time::timeout(limit, rx.wait_for(|p| done(p))).await, Ok(Ok(_)));
    reached
}
