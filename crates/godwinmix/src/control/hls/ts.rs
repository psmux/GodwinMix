//! The MPEG-TS playlists and segments, beside the CMAF ones.
//!
//! | Route | Answer |
//! |---|---|
//! | `GET /hls/{output}/master-ts.m3u8` | the TS multivariant playlist |
//! | `GET /hls/{output}/{rendition}/ts.m3u8` | `main` (the picture and the first sound) or `audio<n>` |
//! | `GET /hls/{output}/{rendition}/{start_ms}.ts` | one segment, named by its wall clock start |
//!
//! Plain HLS whatever `low_latency` says: no parts, no blocking reloads.
//! A segment is named by the UTC millisecond its first frame was made, so
//! a name is never used twice for an output, not after a restart either.

use super::auth::Viewer;
use super::{listing, media_response, refuse, Door, PLAYLIST};
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use godwinmix_core::hls::playlist::with_query;
use godwinmix_core::hls::ts::{self, At};
use godwinmix_core::hls::Stream;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The URI `main` or a further sound has in the TS master.
pub fn rendition_uri(id: Option<&str>) -> String {
    format!("{}/ts.m3u8", id.unwrap_or(ts::MAIN))
}

fn no_ts(stream: &Stream) -> Box<Response> {
    let message = format!(
        "`{}` makes CMAF segments only. Set params.segments to \"ts\" or \"both\" for MPEG-TS; its CMAF playlist is master.m3u8.",
        stream.id
    );
    Box::new(refuse(StatusCode::NOT_FOUND, message, json!({ "output": stream.id, "segments": stream.params.segments.as_str() })))
}

fn open(door: &impl Door, output: &str, req: &Request) -> Result<(Arc<Stream>, Viewer), Box<Response>> {
    let stream = door.find(output, req)?;
    let viewer = super::auth::admit(door, &stream, req)?;
    if !stream.params.segments.ts() {
        return Err(no_ts(&stream));
    }
    Ok((stream, viewer))
}

pub async fn master<D: Door>(State(door): State<D>, Path(output): Path<String>, req: Request) -> Response {
    let (stream, mut viewer) = match open(&door, &output, &req) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    viewer.ensure_id();
    // Asked for before the first segment: wait a little, as the CMAF one does.
    let deadline = Instant::now() + Duration::from_millis(u64::from(stream.params.segment_ms) * 3).max(Duration::from_secs(6));
    while !stream.ts_ready() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    match stream.ts_master() {
        Some(m) => listing(with_query(&m.render(rendition_uri), &viewer.query), PLAYLIST),
        None => super::playlists::not_yet(&stream, "The first MPEG-TS segment"),
    }
}

pub async fn media<D: Door>(State(door): State<D>, Path((output, rung)): Path<(String, String)>, req: Request) -> Response {
    let (stream, viewer) = match open(&door, &output, &req) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    match stream.ts_media(&rung) {
        Some(m) => listing(with_query(&m.render(|e| format!("{}.ts", e.start_ms)), &viewer.query), PLAYLIST),
        None => missing(&stream, &rung),
    }
}

/// `{start_ms}.ts`.
pub fn parse(file: &str) -> Option<i64> {
    let stem = file.strip_suffix(".ts")?;
    stem.bytes().all(|b| b.is_ascii_digit()).then(|| stem.parse().ok()).flatten()
}

/// One segment, already let in by `files::file`.
pub fn segment(stream: &Stream, viewer: &Viewer, rung: &str, start_ms: i64) -> Response {
    if !stream.params.segments.ts() {
        return *no_ts(stream);
    }
    let Some(bytes) = stream.ts_segment(rung, At::Start(start_ms)) else {
        if stream.ts_track(rung).is_none() {
            return missing(stream, rung);
        }
        let message = format!("{rung}/{start_ms}.ts is not in the window. Reload {rung}/ts.m3u8 for the segments there are.");
        return refuse(StatusCode::NOT_FOUND, message, json!({ "output": stream.id, "rung": rung }));
    };
    stream.viewers.served(&viewer.who, bytes.len());
    media_response(vec![bytes], "video/mp2t", stream.params.window_s * 2)
}

fn missing(stream: &Stream, rung: &str) -> Response {
    let have: Vec<String> = stream.ts_tracks().iter().map(|t| t.id.clone()).collect();
    let message = format!("{} has no MPEG-TS rendition `{rung}`. It has {}.", stream.id, have.join(", "));
    refuse(StatusCode::NOT_FOUND, message, json!({ "output": stream.id, "renditions": have }))
}
