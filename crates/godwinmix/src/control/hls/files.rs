//! Init segments, segments and parts.

use super::playlists::wait;
use super::{auth, media_response, refuse, Door};
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use godwinmix_core::hls::playlist::init_uri;
use godwinmix_core::hls::TrackKind;
use serde_json::json;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Name {
    Init(u32),
    Segment(u64),
    Part(u64, u32),
}

/// `init.mp4`, `init3.mp4`, `120.m4s`, `120.2.m4s`.
pub fn parse(file: &str) -> Option<Name> {
    if let Some(gen) = file.strip_prefix("init").and_then(|r| r.strip_suffix(".mp4")) {
        return if gen.is_empty() { Some(Name::Init(0)) } else { gen.parse().ok().map(Name::Init) };
    }
    let stem = file.strip_suffix(".m4s")?;
    match stem.split_once('.') {
        None => stem.parse().ok().map(Name::Segment),
        Some((m, p)) => Some(Name::Part(m.parse().ok()?, p.parse().ok()?)),
    }
}

pub async fn file<D: Door>(State(door): State<D>, Path((output, rung, file)): Path<(String, String, String)>, req: Request) -> Response {
    let stream = match door.find(&output, &req) {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let viewer = match auth::admit(&door, &stream, &req) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let (Some(track), Some(name)) = (stream.track(&rung), parse(&file)) else {
        let message = format!("{output} has no {rung}/{file}. Its playlists name every file there is.");
        return refuse(StatusCode::NOT_FOUND, message, json!({ "output": output, "rung": rung }));
    };
    let p = stream.params;
    let newest = track.position().newest().unwrap_or(0);
    // A player may ask for the part the playlist hinted, or the segment
    // being written, a moment early. Hold those until they exist.
    match name {
        Name::Part(m, i) if m <= newest + 1 => {
            let limit = Duration::from_millis(u64::from(p.segment_ms + 2 * p.part_ms));
            wait(&track, limit, |pos| pos.reached(m, Some(i))).await;
        }
        Name::Segment(m) if m <= newest + 1 => {
            wait(&track, Duration::from_millis(u64::from(p.segment_ms) * 2), |pos| pos.reached(m, None)).await;
        }
        _ => {}
    }
    let found = match name {
        Name::Init(g) => track.init(g).map(|b| vec![b]),
        Name::Segment(m) => track.segment(m),
        Name::Part(m, i) => track.part(m, i).map(|b| vec![b]),
    };
    let Some(parts) = found else {
        let listed: Vec<u64> = track.view().segments.iter().filter(|s| s.complete).map(|s| s.msn).collect();
        let message = format!(
            "{rung}/{file} is not in the window. {rung} holds segments {} to {} and {}; reload the playlist.",
            listed.first().copied().unwrap_or(0),
            listed.last().copied().unwrap_or(0),
            init_uri(0)
        );
        return refuse(StatusCode::NOT_FOUND, message, json!({ "rung": rung, "segments": listed }));
    };
    let bytes = parts.iter().map(|b| b.len()).sum();
    stream.viewers.served(&viewer.who, bytes);
    let kind = if track.kind == TrackKind::Audio { "audio/mp4" } else { "video/mp4" };
    media_response(parts, kind, p.window_s * 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(parse("init.mp4"), Some(Name::Init(0)));
        assert_eq!(parse("init2.mp4"), Some(Name::Init(2)));
        assert_eq!(parse("120.m4s"), Some(Name::Segment(120)));
        assert_eq!(parse("120.3.m4s"), Some(Name::Part(120, 3)));
        for bad in ["index.m3u8", "x.m4s", "1.2.3.m4s", "initx.mp4", "../1.m4s", "1.m4s.m4s"] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }
}
