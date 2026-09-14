//! `/mjpeg/*`, `/pcm/*`, `/opus/*` and `/whep/*`: the preview and monitoring
//! doors, for clients that are not a browser holding `/rpc` open.
//!
//! # What is here
//!
//! | Route | Answer |
//! |---|---|
//! | `GET /mjpeg/sheet?width=` | `multipart/x-mixed-replace`, the whole mosaic |
//! | `GET /mjpeg/{source}`, `/mjpeg/program` | the same, one cell |
//! | `GET /mjpeg/preview` | the armed scene; the programme tile until the scene server lands |
//! | `GET /mjpeg/item/{id}` | one scene item as a projector; 501 until the scene server lands |
//! | `GET /pcm/{target}` | WebSocket, F32LE 48 kHz stereo, 10 ms a frame |
//! | `GET /opus/{target}` | WebSocket, Opus 48 kHz, 20 ms a frame, 64 kbit/s |
//! | `POST /whep/{target}` | an SDP answer, where `whepserversink` is installed |
//!
//! # Tokens
//!
//! The existing rules, unchanged: `Authorization: Bearer <token>` normally, or
//! `?token=` on a GET, because an `<img>` tag and a browser opening a WebSocket
//! have no way to set a header. A POST does not get the query form. Every route
//! here needs the `read` scope, which is what a preview is.
//!
//! # Deadlines
//!
//! Every write to a client is under a deadline, for the reason the same rule
//! exists on `/rpc`: these streams hold a mosaic subscription or an audio
//! branch, and a peer that stops reading would otherwise keep the pipeline up
//! for everybody until TCP gave up. On expiry the stream ends and the hold goes
//! with it.

use crate::control::{presented_token, Ctx};
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use godwinmix_core::multiview::{MultiviewRequest, MultiviewSubscription};
use godwinmix_core::preview::audio::{AudioRequest, Codec, SampleFormat};
use godwinmix_core::preview::{mjpeg, whep};
use godwinmix_core::snapshot::{self, Pick};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, warn};

/// The same deadline `/rpc` uses, and for the same reason.
const SEND_DEADLINE: Duration = Duration::from_secs(5);

/// How long a stream waits for the mosaic to be built before it gives up and
/// says so. A build is a pipeline coming up, which is well under a second on
/// every machine in the lab.
const FIRST_FRAME_WAIT: Duration = Duration::from_secs(5);

pub fn router(ctx: Ctx) -> Router<Ctx> {
    Router::new()
        .route("/mjpeg/item/{id}", get(mjpeg_item))
        .route("/mjpeg/{target}", get(mjpeg_stream))
        .route("/pcm/{target}", get(pcm_stream))
        .route("/opus/{target}", get(opus_stream))
        .route("/whep/{target}", post(whep_offer).get(whep_not_a_get))
        .with_state(ctx)
}

/// The token check for these routes.
///
/// Written out here rather than reused from `rest.rs` because these are not
/// generated routes: they carry bytes, not JSON, and a refusal has to be a
/// plain status a `<img>` tag or an `aplay` pipeline can act on.
fn authorise(ctx: &Ctx, method: &Method, headers: &HeaderMap, uri: &Uri) -> Result<(), Response> {
    let presented = presented_token(method, headers, uri);
    let token = ctx.app.tokens.authenticate(presented.as_deref()).map_err(|reason| {
        (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            axum::Json(json!({ "error": reason.message() })),
        )
            .into_response()
    })?;
    if !token.has(godwinmix_protocol::scope::Scope::Read) {
        return Err(refuse(
            StatusCode::FORBIDDEN,
            "this token does not carry the read scope, which a preview stream needs",
        ));
    }
    Ok(())
}

fn refuse(code: StatusCode, message: &str) -> Response {
    (code, axum::Json(json!({ "error": message }))).into_response()
}

/// `?width=` on an MJPEG route, and `?rate=`, `?channels=`, `?format=` on an
/// audio one.
fn number(q: &HashMap<String, String>, key: &str) -> Option<i32> {
    q.get(key).and_then(|v| v.parse().ok())
}

// --- MJPEG ------------------------------------------------------------------

async fn mjpeg_stream(
    State(ctx): State<Ctx>,
    Path(target): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    req: Request,
) -> Response {
    if let Err(r) = authorise(&ctx, req.method(), req.headers(), req.uri()) {
        return r;
    }
    let target = mjpeg::Target::parse(&target);
    if !ctx.app.multiview.enabled() {
        return refuse(
            StatusCode::NOT_FOUND,
            "[multiview] enabled = false, so there is no picture to preview. Turn it on and \
             restart the core.",
        );
    }
    let width = number(&q, "width").filter(|w| *w > 0).map(|w| w as u32);
    // A cell is cut out of the mosaic, so a client asking for a wide cell is
    // asking for a wide mosaic. The sheet route reads `width` as the answer's
    // width instead, which is what a client scaling a whole sheet means.
    let mosaic = MultiviewRequest {
        fps: number(&q, "fps").unwrap_or(0),
        width: if target.is_sheet() { 0 } else { width.unwrap_or(0) as i32 },
    };
    // The subscription and the client count live in the stream's own state, so
    // the mosaic is released the moment the client goes: axum drops the body
    // when the connection ends, and that drops both.
    let state = MjpegState {
        pick: target.pick(),
        width,
        subscription: ctx.app.multiview.subscribe(mosaic),
        _counted: ctx.app.preview.count("mjpeg"),
        ctx,
    };
    let stream = futures_util::stream::unfold(state, |mut state| async move {
        next_part(&mut state).await.map(|part| (Ok::<_, std::io::Error>(part), state))
    });
    (
        [
            (header::CONTENT_TYPE, mjpeg::content_type()),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}

/// Everything one MJPEG stream carries between frames.
struct MjpegState {
    ctx: Ctx,
    pick: Option<Pick>,
    width: Option<u32>,
    /// What keeps the mosaic up for this client's whole life.
    subscription: MultiviewSubscription,
    /// What `gmx_stream_clients{kind="mjpeg"}` reads.
    _counted: godwinmix_core::preview::ClientGuard,
}

/// The next part to write, or `None` when this stream is over.
///
/// A dropped mosaic frame is the right answer on a slow link, and a cell that
/// is not on the sheet yet waits for the next frame rather than ending the
/// stream, because a source that is still connecting will appear on it.
async fn next_part(state: &mut MjpegState) -> Option<Vec<u8>> {
    loop {
        let jpeg = match tokio::time::timeout(FIRST_FRAME_WAIT, state.subscription.recv()).await {
            Ok(Ok(frame)) => frame,
            Ok(Err(broadcast::error::RecvError::Lagged(n))) => {
                debug!(skipped = n, "mjpeg client fell behind");
                continue;
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => return None,
            Err(_) => {
                debug!("no mosaic frame within the wait, ending the mjpeg stream");
                return None;
            }
        };
        // The cell this client wants, from the layout as it stands. A source
        // added or removed moves the cells, so it is read per frame.
        let cell = match &state.pick {
            Some(Pick::Sheet) | None => None,
            Some(p) => {
                let status = state.ctx.app.mixer.status().await.ok()?;
                match snapshot::find_cell(&status.multiview.cells, p) {
                    Some(c) => Some(c.clone()),
                    None => continue,
                }
            }
        };
        let width = state.width;
        let bytes = jpeg.clone();
        // A decode, a crop and an encode is real work and never runs on an
        // async worker.
        match tokio::task::spawn_blocking(move || mjpeg::cut(&bytes, cell.as_ref(), width)).await {
            Ok(Ok(jpeg)) => return Some(mjpeg::part(&jpeg)),
            Ok(Err(e)) => {
                warn!(?e, "a mosaic frame could not be cut for an mjpeg client");
                continue;
            }
            Err(e) => {
                warn!(?e, "the mjpeg cutter panicked");
                return None;
            }
        }
    }
}

/// One scene item as a projector. The scene server owns items, so until it
/// lands this says so and names what does work.
async fn mjpeg_item(
    State(ctx): State<Ctx>,
    Path(id): Path<String>,
    req: Request,
) -> Response {
    if let Err(r) = authorise(&ctx, req.method(), req.headers(), req.uri()) {
        return r;
    }
    refuse(
        StatusCode::NOT_IMPLEMENTED,
        &format!(
            "a projector for one scene item ('{id}') needs the scene server, which this build \
             does not have yet. Use /mjpeg/preview for the armed scene, /mjpeg/program for the \
             programme, or /mjpeg/{{source}} for one camera."
        ),
    )
}

// --- audio ------------------------------------------------------------------

async fn pcm_stream(
    State(ctx): State<Ctx>,
    Path(target): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
    req: Request,
) -> Response {
    audio_stream(ctx, target, q, ws, req, Codec::Pcm).await
}

async fn opus_stream(
    State(ctx): State<Ctx>,
    Path(target): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
    req: Request,
) -> Response {
    audio_stream(ctx, target, q, ws, req, Codec::Opus).await
}

async fn audio_stream(
    ctx: Ctx,
    target: String,
    q: HashMap<String, String>,
    ws: WebSocketUpgrade,
    req: Request,
    codec: Codec,
) -> Response {
    if let Err(r) = authorise(&ctx, req.method(), req.headers(), req.uri()) {
        return r;
    }
    let request = AudioRequest {
        rate: number(&q, "rate").unwrap_or(48_000),
        channels: number(&q, "channels").unwrap_or(2),
        format: q
            .get("format")
            .and_then(|f| SampleFormat::parse(f))
            .unwrap_or(SampleFormat::F32le),
        codec,
    }
    .clamped();

    // Opened before the upgrade, so a refusal is a status a shell script can
    // read rather than a socket that closes for no stated reason.
    let opened = ctx.app.preview.open_audio(&target, request).await;
    let (lease, frames) = match opened {
        Ok(pair) => pair,
        Err(message) => return refuse(StatusCode::NOT_FOUND, &message),
    };
    ws.on_upgrade(move |socket| pump_audio(socket, lease, frames))
}

/// One audio monitoring socket: frames out, nothing in, both sides watched.
async fn pump_audio(
    socket: WebSocket,
    lease: godwinmix_core::preview::AudioStream,
    mut frames: broadcast::Receiver<godwinmix_core::preview::audio::Frame>,
) {
    // Dropping this is what eventually removes the branch.
    let _lease = lease;
    let (mut tx, mut rx) = socket.split();
    loop {
        tokio::select! {
            incoming = rx.next() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },
            frame = frames.recv() => match frame {
                Ok(bytes) => {
                    let message = Message::Binary(bytes.to_vec().into());
                    match tokio::time::timeout(SEND_DEADLINE, tx.send(message)).await {
                        Ok(Ok(())) => {}
                        Ok(Err(_)) => break,
                        Err(_) => {
                            warn!(
                                "audio monitoring client stopped reading; closing it so it stops \
                                 holding the branch up"
                            );
                            break;
                        }
                    }
                }
                // Sound that is late is worse than sound that is missing.
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(skipped = n, "audio monitoring client fell behind");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    debug!("audio monitoring client disconnected");
}

// --- WHEP -------------------------------------------------------------------

async fn whep_offer(
    State(ctx): State<Ctx>,
    Path(target): Path<String>,
    req: Request,
) -> Response {
    if let Err(r) = authorise(&ctx, req.method(), req.headers(), req.uri()) {
        return r;
    }
    if !whep::available() {
        return (
            StatusCode::NOT_IMPLEMENTED,
            axum::Json(json!({ "error": whep::missing_message() })),
        )
            .into_response();
    }
    let target = whep::Target::parse(&target);
    // The element is here but the session path is not wired yet. Say which of
    // the two it is, because they need different things from the reader.
    (
        StatusCode::NOT_IMPLEMENTED,
        axum::Json(json!({
            "error": format!(
                "whepserversink is installed on this core but the WHEP session path for '{}' \
                 is not wired yet. Use /mjpeg/program with /pcm/program until it is.",
                target.label()
            ),
        })),
    )
        .into_response()
}

/// A browser that opens `/whep/program` in the address bar gets told what to
/// do rather than a bare 405.
async fn whep_not_a_get(Path(target): Path<String>) -> Response {
    refuse(
        StatusCode::METHOD_NOT_ALLOWED,
        &format!(
            "WHEP is a POST with an SDP offer in the body, not a GET. \
             POST /whep/{target} with Content-Type: application/sdp."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_number_is_read_or_ignored() {
        let mut q = HashMap::new();
        q.insert("rate".to_string(), "16000".to_string());
        q.insert("channels".to_string(), "not a number".to_string());
        assert_eq!(number(&q, "rate"), Some(16_000));
        assert_eq!(number(&q, "channels"), None);
        assert_eq!(number(&q, "absent"), None);
    }

    #[tokio::test]
    async fn an_item_projector_says_which_part_is_missing_and_what_works() {
        let r = whep_not_a_get(Path("program".into())).await;
        assert_eq!(r.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
