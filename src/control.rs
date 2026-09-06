//! HTTP and WebSocket control plane.
//!
//! The UI is a plain web app served by this binary, which is what makes the
//! Tauri desktop app and remote browser control the same program. Tauri points
//! a webview at this server; a remote operator points a browser at it. There
//! is no second implementation to keep in step.
//!
//! One WebSocket carries everything the UI needs: JSON text frames for state
//! and events, binary frames for mosaic JPEGs. One connection, one port, which
//! matters when the only way in is a firewall rule somebody else has to write.

use crate::config::{OutputConfig, SourceConfig};
use crate::media::{MediaLibrary, MediaListing};
use crate::mixer::{Command, MixerHandle};
use crate::state::{Event, MixerStatus};
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{debug, info, warn};

const UI: &str = include_str!("../ui/index.html");

#[derive(Clone)]
pub struct AppState {
    pub mixer: MixerHandle,
    /// Mosaic frames. `None` when multiview is disabled.
    pub frames: Option<Arc<broadcast::Sender<Arc<[u8]>>>>,
    /// Ad clips available on this machine.
    pub library: Arc<MediaLibrary>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/status", get(status))
        .route("/api/take", post(take))
        .route("/api/media", get(list_media))
        .route("/api/adbreak", post(start_ad_break))
        .route("/api/adbreak/end", post(end_ad_break))
        .route("/api/sources", post(add_source))
        .route("/api/sources/{id}", delete(remove_source))
        .route("/api/outputs", get(list_outputs).post(add_output))
        .route("/api/outputs/{id}", delete(remove_output))
        .route("/api/outputs/{id}/reconnect", post(reconnect_output))
        .route("/ws", get(ws_upgrade))
        // The Tauri shell and a browser on another origin both need this.
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(UI)
}

/// Anything that goes wrong becomes a 400 with the message. The UI shows it
/// verbatim, because "no such source cam9" is more use to an operator than a
/// generic failure.
struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        warn!(error = %self.0, "request failed");
        (StatusCode::BAD_REQUEST, self.0.to_string()).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}

async fn status(State(app): State<AppState>) -> Result<Json<MixerStatus>, ApiError> {
    Ok(Json(app.mixer.status().await?))
}

#[derive(Debug, Deserialize)]
struct TakeRequest {
    /// Omit or null to cut to the slate.
    #[serde(default)]
    source: Option<String>,
    /// Running time to land the cut on. Omit for immediate.
    #[serde(default)]
    at_running_time_ms: Option<u64>,
}

async fn take(
    State(app): State<AppState>,
    Json(req): Json<TakeRequest>,
) -> Result<StatusCode, ApiError> {
    app.mixer
        .request(|ack| Command::Take {
            source: req.source,
            at_running_time_ms: req.at_running_time_ms,
            ack: Some(ack),
        })
        .await?;
    Ok(StatusCode::OK)
}

/// Scanning opens and demuxes files, so it runs off the async workers.
async fn list_media(State(app): State<AppState>) -> Result<Json<MediaListing>, ApiError> {
    let library = app.library.clone();
    let listing = tokio::task::spawn_blocking(move || library.list())
        .await
        .map_err(|e| anyhow::anyhow!("media scan failed: {e}"))?;
    Ok(Json(listing))
}

#[derive(Debug, Deserialize)]
struct AdBreakRequest {
    /// File path or URI of the ad to play.
    uri: String,
    /// Running time to open the break on. Omit to roll immediately.
    #[serde(default)]
    at_running_time_ms: Option<u64>,
    /// Source to rejoin afterwards. Omit to return to whatever is on program.
    #[serde(default)]
    return_to: Option<String>,
}

async fn start_ad_break(
    State(app): State<AppState>,
    Json(req): Json<AdBreakRequest>,
) -> Result<StatusCode, ApiError> {
    app.mixer
        .request(|ack| Command::AdBreak {
            uri: req.uri,
            at_running_time_ms: req.at_running_time_ms,
            return_to: req.return_to,
            ack: Some(ack),
        })
        .await?;
    Ok(StatusCode::OK)
}

async fn end_ad_break(State(app): State<AppState>) -> Result<StatusCode, ApiError> {
    app.mixer.request(|ack| Command::EndAdBreak(Some(ack))).await?;
    Ok(StatusCode::OK)
}

/// What the UI and the CLI send to add a source. Everything but the URL is
/// optional: a person pasting a link should not have to invent an id or know
/// the mixer's URL prefixes.
#[derive(Debug, Deserialize)]
struct AddSourceRequest {
    /// Stable id used by `take` and the API. Derived from the name or the
    /// host when omitted, made unique with a numeric suffix if needed.
    #[serde(default)]
    id: Option<String>,
    /// Name shown in the UI. Defaults to the host of the URL.
    #[serde(default)]
    name: Option<String>,
    uri: String,
    /// "web" renders the URL as a page in the browser sidecar (the same as
    /// writing `web+` in front of it); "auto" or omitted works the protocol
    /// out from the URL.
    #[serde(default)]
    kind: Option<String>,
    /// "off" or "auto", the same spellings the config file uses. "auto" lets
    /// the mixer decode the page's own video itself and draw the page over
    /// the top, when the page turns out to have an address worth handing
    /// over. See `Superimpose`.
    ///
    /// Only a website source ever reads it. It is accepted on any source and
    /// ignored by the rest rather than refused, because the field is part of
    /// every `SourceConfig` and a camera simply never consults it. Refusing
    /// would mean the API knowing which URLs are pages, which is the one
    /// thing this endpoint deliberately works out later.
    #[serde(default)]
    superimpose: Option<String>,
}

async fn add_source(
    State(app): State<AppState>,
    Json(req): Json<AddSourceRequest>,
) -> Result<StatusCode, ApiError> {
    let uri = req.uri.trim().to_string();
    if uri.is_empty() {
        return Err(anyhow::anyhow!("a source needs a URL").into());
    }
    let uri = match req.kind.as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some("web") | Some("page") | Some("website") => crate::input::as_web_uri(&uri),
        _ => uri,
    };
    let name = req.name.filter(|n| !n.trim().is_empty());
    // Spelled out rather than left off. `SourceConfig::superimpose` has a
    // serde default, but a default fills in for a missing key and not for a
    // null one, and the UI sends null for the sources this cannot apply to.
    let superimpose = req
        .superimpose
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "off".to_string());
    let base_id = req
        .id
        .filter(|i| !i.trim().is_empty())
        .unwrap_or_else(|| slug(name.as_deref().unwrap_or(&host_of(&uri))));
    // A derived id may collide with an existing source; try a few suffixes.
    let candidates = std::iter::once(base_id.clone()).chain((2..10).map(|n| format!("{base_id}-{n}")));
    for id in candidates {
        let cfg: SourceConfig = serde_json::from_value(serde_json::json!({
            "id": id, "uri": uri, "name": name, "superimpose": superimpose,
        }))
        .map_err(|e| anyhow::anyhow!("bad source: {e}"))?;
        match app
            .mixer
            .request(|ack| Command::AddSource(Box::new(cfg), Some(ack)))
            .await
        {
            Ok(()) => return Ok(StatusCode::OK),
            Err(e) if e.to_string().contains("already exists") => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(anyhow::anyhow!("could not find a free id for {base_id}").into())
}

/// The host part of a URL, or the whole thing when it has none; used to name
/// and identify sources people add by pasting a link.
fn host_of(uri: &str) -> String {
    let u = uri.trim();
    let u = u.strip_prefix("web+").unwrap_or(u);
    let rest = u.split_once("://").map(|(_, r)| r).unwrap_or(u);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.is_empty() { "source".into() } else { host.to_string() }
}

/// An id from free text: lowercase, ascii letters and digits, dashes between
/// words, nothing else.
fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "source".into() } else { out }
}

async fn remove_source(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    app.mixer
        .request(|ack| Command::RemoveSource(id, Some(ack)))
        .await?;
    Ok(StatusCode::OK)
}

async fn reconnect_output(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    app.mixer
        .request(|ack| Command::ReconnectOutput(id, Some(ack)))
        .await?;
    Ok(StatusCode::OK)
}

async fn list_outputs(
    State(app): State<AppState>,
) -> Result<Json<Vec<crate::state::OutputStatus>>, ApiError> {
    Ok(Json(app.mixer.status().await?.outputs))
}

async fn add_output(
    State(app): State<AppState>,
    Json(cfg): Json<OutputConfig>,
) -> Result<StatusCode, ApiError> {
    app.mixer
        .request(|ack| Command::AddOutput(Box::new(cfg), Some(ack)))
        .await?;
    Ok(StatusCode::OK)
}

async fn remove_output(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    app.mixer
        .request(|ack| Command::RemoveOutput(id, Some(ack)))
        .await?;
    Ok(StatusCode::OK)
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(app): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| serve_ws(socket, app))
}

async fn serve_ws(socket: WebSocket, app: AppState) {
    let (mut tx, mut rx) = socket.split();

    // Send a snapshot first so a UI that connects mid-broadcast, or reconnects
    // after a dropout, can rebuild its whole view without any replay.
    match app.mixer.status().await {
        Ok(s) => {
            let ev = Event::Status(Box::new(s));
            if let Ok(json) = serde_json::to_string(&ev) {
                if tx.send(Message::Text(json.into())).await.is_err() {
                    return;
                }
            }
        }
        Err(e) => {
            warn!(?e, "could not snapshot state for new websocket client");
            return;
        }
    }

    let mut events = app.mixer.subscribe();
    let mut frames = app.frames.as_ref().map(|f| f.subscribe());

    loop {
        tokio::select! {
            // Client closed, or sent something. We accept no commands here;
            // control goes over HTTP so that failures get a status code.
            incoming = rx.next() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },

            ev = events.recv() => match ev {
                Ok(ev) => {
                    let Ok(json) = serde_json::to_string(&ev) else { continue };
                    if tx.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(skipped = n, "websocket client fell behind on events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },

            // `frames` is None when multiview is off, in which case this arm
            // is disabled and the select just handles events.
            frame = async {
                match frames.as_mut() {
                    Some(f) => f.recv().await,
                    None => std::future::pending().await,
                }
            } => match frame {
                Ok(bytes) => {
                    if tx.send(Message::Binary(bytes.to_vec().into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    // Expected on a slow link. Dropping preview frames is the
                    // correct response; the newest one is along shortly.
                    debug!(skipped = n, "websocket client fell behind on preview frames");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    debug!("websocket client disconnected");
}

pub async fn serve(bind: &str, state: AppState) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(%bind, "control server listening");
    axum::serve(listener, router(state)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Superimpose;

    /// `add_source` builds its `SourceConfig` through JSON, so the strings the
    /// API accepts have to be exactly the ones serde knows. A spelling that
    /// drifts, "on" or `true`, would be a 400 on a field that reads as
    /// obviously correct to whoever sent it.
    #[test]
    fn superimpose_spellings_survive_the_trip_through_json() {
        let cfg = |v: serde_json::Value| -> Result<SourceConfig, _> {
            serde_json::from_value(serde_json::json!({
                "id": "page", "uri": "web+https://example.com/live", "name": null,
                "superimpose": v,
            }))
        };
        assert_eq!(cfg("auto".into()).unwrap().superimpose, Superimpose::Auto);
        assert_eq!(cfg("off".into()).unwrap().superimpose, Superimpose::Off);
        assert!(cfg("on".into()).is_err());
        // Why the handler substitutes "off" rather than passing a null on:
        // the serde default fills in a missing key, not a null one.
        assert!(cfg(serde_json::Value::Null).is_err());
        // And omitting it entirely is the default, which is today's behaviour.
        let bare: SourceConfig = serde_json::from_value(serde_json::json!({
            "id": "page", "uri": "web+https://example.com/live",
        }))
        .unwrap();
        assert_eq!(bare.superimpose, Superimpose::Off);
    }

    #[test]
    fn ids_are_derived_from_hosts_and_names() {
        assert_eq!(host_of("https://www.youtube.com/watch?v=x"), "youtube.com");
        assert_eq!(host_of("web+https://user:pw@host.tv:8443/live"), "host.tv");
        assert_eq!(host_of("rtmp://127.0.0.1:1935/live/cam1"), "127.0.0.1");
        assert_eq!(slug("youtube.com"), "youtube-com");
        assert_eq!(slug("  Camera #2 (wide) "), "camera-2-wide");
        assert_eq!(slug("***"), "source");
    }
}
