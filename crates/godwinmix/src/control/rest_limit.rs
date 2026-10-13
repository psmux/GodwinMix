//! How big a JSON body `/api/v1` reads, and the answer to one that is bigger.
//!
//! A body over the limit used to be a 400 that said "could not read the body:
//! length limit exceeded", and through a station it was worse: the show
//! answered before the rest of the body had arrived, closed the connection,
//! and the station's relay reported a 502 for a show that was perfectly well.
//! Now the answer is 413 with a sentence naming the limit, and the rest of the
//! body is read and thrown away first (up to [`DRAIN`]) so the client and any
//! relay in between are still listening when it comes.

use axum::body::Body;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use godwinmix_protocol::error::{ErrorCode, RpcError};

/// What every other call is held to: a few hundred bytes is usual.
pub const DEFAULT: usize = 1 << 20;

/// `tool.call` arguments go to the plugin as one protocol line, so they may
/// be as long as a line may: 4 MiB, docs/reference/plugin-protocol.md.
pub const TOOL_CALL: usize = godwinmix_protocol::plugin::wire::MAX_LINE_BYTES;

/// How much of an over long body is read and dropped so the 413 reaches the
/// client. Past this the connection is closed instead.
pub const DRAIN: usize = 64 << 20;

/// The limit for one method's body.
pub fn for_method(method: &str, max_upload: usize) -> usize {
    match method {
        // A graphic carries its picture or clip as base64, so these take what
        // an upload takes.
        "gallery.save" | "gallery.import" => max_upload.saturating_mul(4) / 3 + 4096,
        "tool.call" => TOOL_CALL,
        _ => DEFAULT,
    }
}

/// Why a body was not read.
pub enum Unread {
    TooLarge,
    Failed(String),
}

/// The whole body, up to `limit` bytes. Past it, what is left is drained.
pub async fn read(headers: &HeaderMap, body: Body, limit: usize) -> Result<axum::body::Bytes, Unread> {
    let declared = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()?.parse::<usize>().ok());
    let mut stream = body.into_data_stream();
    let mut kept = Vec::with_capacity(declared.unwrap_or(0).min(limit));
    let mut seen = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Unread::Failed(e.to_string()))?;
        seen += chunk.len();
        if seen > limit.saturating_add(DRAIN) {
            break;
        }
        if seen <= limit {
            kept.extend_from_slice(&chunk);
        }
    }
    match seen > limit {
        true => Err(Unread::TooLarge),
        false => Ok(kept.into()),
    }
}

/// 413, saying the limit and what to do instead.
pub fn too_large(method: &str, limit: usize, trace_id: &str) -> Response {
    let e = RpcError::new(
        ErrorCode::InvalidParams,
        format!(
            "the body of {method} is over its limit of {} bytes ({} MiB). Send less in one call: \
             a file goes up with media.upload and is named by its path.",
            limit,
            limit >> 20
        ),
    )
    .with("limit_bytes", limit);
    let headers = [(header::HeaderName::from_static("x-trace-id"), trace_id.to_string())];
    (StatusCode::PAYLOAD_TOO_LARGE, headers, Json(e.body(trace_id))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_body_one_byte_over_is_refused_and_one_at_the_limit_is_read() {
        let at = read(&HeaderMap::new(), Body::from(vec![b'x'; 10]), 10).await;
        assert!(matches!(at, Ok(b) if b.len() == 10));
        let over = read(&HeaderMap::new(), Body::from(vec![b'x'; 11]), 10).await;
        assert!(matches!(over, Err(Unread::TooLarge)));
        assert_eq!(for_method("tool.call", 0), 4 * 1024 * 1024);
        assert_eq!(too_large("tool.call", TOOL_CALL, "t").status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
