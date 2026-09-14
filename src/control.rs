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
//!
//! Alongside the UI's endpoints sit a few meant for software rather than
//! people: still snapshots cut from the mosaic and a compact state document.
//! They exist so that an AI agent can look at the mixer for the price of one
//! small request rather than a video stream. See `snapshot.rs`.

use crate::config::{OutputConfig, SnapshotConfig, SourceConfig};
use crate::media::{MediaLibrary, MediaListing};
use crate::mixer::{AudioOutcome, Command, MixerHandle, SeekOutcome};
use crate::multiview::{MultiviewHandle, MultiviewRequest};
use crate::snapshot::{self, Ask, Pick, Tracker};
use crate::state::{Event, MixerStatus, SourceState};
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, FromRef, Path, Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{debug, info, warn};

const UI: &str = include_str!("../ui/index.html");

#[derive(Clone)]
pub struct AppState {
    pub mixer: MixerHandle,
    /// The mosaic. Subscribing through this is what builds it; there is no
    /// other way to reach the frames. See `multiview.rs`.
    pub multiview: MultiviewHandle,
    /// The still and motion limits, `[snapshot]` in the config.
    pub snapshot: SnapshotConfig,
    /// Ad clips available on this machine.
    pub library: Arc<MediaLibrary>,
    /// Runs file transcodes and remembers their progress.
    pub converter: Arc<crate::convert::Converter>,
    /// Rung by `POST /api/shutdown`. `main` waits on it alongside Ctrl-C and
    /// takes the whole process down the same way for either.
    pub quit: Arc<tokio::sync::Notify>,
    /// Bearer token the API and the WebSocket demand. `None` leaves them open,
    /// which is the original behaviour and right for a mixer that only
    /// listens on a LAN nobody else is on.
    pub token: Option<Arc<str>>,
}

/// What the router actually carries: the shared `AppState` plus the snapshot
/// tracker, which is started by `serve` because it needs a running runtime and
/// the mosaic broadcast, and nothing outside this module needs to know it
/// exists. `FromRef` lets every existing handler keep asking for `AppState`.
#[derive(Clone)]
struct Ctx {
    app: AppState,
    snapshots: Arc<Tracker>,
}

impl FromRef<Ctx> for AppState {
    fn from_ref(ctx: &Ctx) -> Self {
        ctx.app.clone()
    }
}

impl FromRef<Ctx> for Arc<Tracker> {
    fn from_ref(ctx: &Ctx) -> Self {
        ctx.snapshots.clone()
    }
}

pub fn router(app: AppState, snapshots: Arc<Tracker>) -> Router {
    let max_upload = app.library.cfg().max_upload_bytes;
    let state = Ctx { app, snapshots };
    // Everything that reads or drives the mixer sits behind the token. The
    // page at `/` does not: it is the same for everyone, contains nothing
    // secret, and is where a browser finds out that it needs a token at all.
    let guarded = Router::new()
        .route("/api/status", get(status))
        .route("/api/agent/state", get(agent_state))
        .route("/api/snapshot/{name}", get(snapshot_image))
        .route("/api/take", post(take))
        .route("/api/golive", post(golive))
        .route("/api/shutdown", post(shutdown))
        .route("/api/media", get(list_media))
        .route(
            "/api/media/upload",
            post(upload_media).layer(DefaultBodyLimit::max(max_upload)),
        )
        .route("/api/media/{name}/convert", post(convert_media))
        .route("/api/media/{name}", delete(delete_media))
        .route("/api/adbreak", post(start_ad_break))
        .route("/api/adbreak/end", post(end_ad_break))
        .route("/api/sources", post(add_source))
        .route("/api/sources/{id}", delete(remove_source))
        .route("/api/sources/{id}/audio", post(set_source_audio))
        .route("/api/sources/{id}/seek", post(seek_source))
        .route("/api/outputs", get(list_outputs).post(add_output))
        .route("/api/outputs/{id}", delete(remove_output))
        .route("/api/outputs/{id}/reconnect", post(reconnect_output))
        .route("/ws", get(ws_upgrade))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));
    Router::new()
        .route("/", get(index))
        .merge(guarded)
        // The Tauri shell and a browser on another origin both need this. It
        // sits outside the token check so that a preflight, which carries no
        // Authorization header by design, is answered rather than refused.
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(UI)
}

/// Turn away a request without the token. Does nothing when none is set.
async fn require_token(State(app): State<AppState>, req: Request, next: Next) -> Response {
    match token_check(app.token.as_deref(), req.method(), req.headers(), req.uri()) {
        Ok(()) => next.run(req).await,
        Err(reason) => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            Json(json!({ "error": reason })),
        )
            .into_response(),
    }
}

/// Whether a request carries the token. `Authorization: Bearer <token>` is
/// the normal form. A GET may put it in the query as `token=` instead,
/// because a browser opening a WebSocket has no way to set a header. Other
/// methods do not get the query form: they come from code that can set the
/// header, and a token in a POST's URL ends up in more logs than it should.
fn token_check(
    expected: Option<&str>,
    method: &Method,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<(), &'static str> {
    let Some(expected) = expected else { return Ok(()) };
    let presented =
        bearer_token(headers).or_else(|| if method == Method::GET { query_token(uri) } else { None });
    match presented {
        None => Err("missing token"),
        Some(t) if constant_time_eq(t.as_bytes(), expected.as_bytes()) => Ok(()),
        Some(_) => Err("wrong token"),
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn query_token(uri: &Uri) -> Option<String> {
    let Query(pairs) = Query::<Vec<(String, String)>>::try_from_uri(uri).ok()?;
    pairs.into_iter().find(|(k, _)| k == "token").map(|(_, v)| v).filter(|v| !v.is_empty())
}

/// Compare without stopping at the first difference, so how long the check
/// takes says nothing about how much of a guess was right. The length goes
/// into the same accumulator rather than being tested up front.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(x ^ y);
    }
    std::hint::black_box(diff) == 0
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
    let converter = app.converter.clone();
    let listing = tokio::task::spawn_blocking(move || library.list_with(Some(&converter)))
        .await
        .map_err(|e| anyhow::anyhow!("media scan failed: {e}"))?;
    Ok(Json(listing))
}

#[derive(Debug, Deserialize)]
struct UploadQuery {
    name: String,
}

/// Stream an uploaded file to disk. Never buffered: a large clip must cost a
/// chunk of memory, not its whole size, and the process has a live programme
/// in it. Written under a dotted `.part` name and renamed on success so a half
/// uploaded file never appears in the listing and never gets taken to air.
async fn upload_media(
    State(app): State<AppState>,
    Query(q): Query<UploadQuery>,
    body: Body,
) -> Result<Json<serde_json::Value>, ApiError> {
    use tokio::io::AsyncWriteExt;
    if !app.library.cfg().allow_upload {
        return Err(anyhow::anyhow!("uploads are disabled on this server").into());
    }
    let name = crate::media::safe_upload_name(&q.name)?;
    let dir = app.library.dir().to_path_buf();
    let part = dir.join(format!(".{name}.part"));
    let final_path = dir.join(&name);

    let mut file = tokio::fs::File::create(&part)
        .await
        .map_err(|e| anyhow::anyhow!("creating {}: {e}", part.display()))?;
    let mut stream = body.into_data_stream();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                drop(file);
                let _ = tokio::fs::remove_file(&part).await;
                return Err(anyhow::anyhow!("upload interrupted: {e}").into());
            }
        };
        written += chunk.len() as u64;
        if let Err(e) = file.write_all(&chunk).await {
            drop(file);
            let _ = tokio::fs::remove_file(&part).await;
            return Err(anyhow::anyhow!("writing upload: {e}").into());
        }
    }
    file.flush().await.ok();
    file.sync_all().await.ok();
    drop(file);
    tokio::fs::rename(&part, &final_path)
        .await
        .map_err(|e| anyhow::anyhow!("finishing upload: {e}"))?;

    info!(%name, bytes = written, "media uploaded");
    app.mixer.emit(crate::state::Event::MediaChanged { name: name.clone(), conversion: None });
    Ok(Json(json!({
        "name": name,
        "path": final_path.display().to_string(),
        "size_bytes": written,
    })))
}

/// Start a background transcode of a library file to a web safe copy.
async fn convert_media(
    State(app): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<crate::convert::ConversionState>, ApiError> {
    let input = app.library.resolve(&name)?;
    let state = app.converter.start(name, input)?;
    Ok(Json(state))
}

/// Delete a library file and its converted copy. Refused while the file is a
/// live source, the one way this could take the show off air.
async fn delete_media(
    State(app): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = app.library.resolve(&name)?;
    let target = crate::input::to_uri(&path.display().to_string());
    let configs = app.mixer.configs().await?;
    if let Some(s) = configs.sources.iter().find(|s| crate::input::to_uri(&s.uri) == target) {
        return Err(anyhow::anyhow!("{name} is the source \"{}\". Remove the source first.", s.id).into());
    }
    let mut removed = Vec::new();
    for p in [path.clone(), crate::convert::converted_sibling(&path)] {
        if p.exists() && std::fs::remove_file(&p).is_ok() {
            removed.push(p.display().to_string());
        }
    }
    app.mixer.emit(crate::state::Event::MediaChanged { name, conversion: None });
    Ok(Json(json!({ "removed": removed })))
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

/// Stop the mixer, and with it the programme. Deliberately a separate call
/// from anything the UI does by itself: closing a window must never take the
/// stream down, so only the desktop shell's "Quit and stop the mixer" and the
/// CLI send this.
async fn shutdown(State(app): State<AppState>) -> StatusCode {
    info!("shutdown requested over the API");
    app.quit.notify_one();
    StatusCode::ACCEPTED
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
    let base_id = req.id.filter(|i| !i.trim().is_empty()).unwrap_or_else(|| match &name {
        Some(n) => slug(n),
        None => derived_id(&uri),
    });
    create_source(&app, base_id, uri, name, &superimpose).await?;
    Ok(StatusCode::OK)
}

/// Add a source under `base_id` or the first free suffix of it, and say which
/// id it got. A derived id can easily collide: two pages on the same host
/// both want to be called after it.
async fn create_source(
    app: &AppState,
    base_id: String,
    uri: String,
    name: Option<String>,
    superimpose: &str,
) -> Result<String> {
    for id in id_candidates(&base_id) {
        let cfg: SourceConfig = serde_json::from_value(json!({
            "id": id, "uri": uri, "name": name, "superimpose": superimpose,
        }))
        .map_err(|e| anyhow::anyhow!("bad source: {e}"))?;
        match app
            .mixer
            .request(|ack| Command::AddSource(Box::new(cfg), Some(ack)))
            .await
        {
            Ok(()) => return Ok(id),
            Err(e) if e.to_string().contains("already exists") => continue,
            Err(e) => return Err(e),
        }
    }
    anyhow::bail!("could not find a free id for {base_id}")
}

/// Same again for an output, named after the host it sends to. The reconnect
/// policy is the default one; golive has no way to ask for another, and a
/// caller who cares can add the output through `/api/outputs` first.
async fn create_output(app: &AppState, uri: &str) -> Result<String> {
    let base_id = derived_id(uri);
    for id in id_candidates(&base_id) {
        let cfg: OutputConfig = serde_json::from_value(json!({ "id": id, "uri": uri }))
            .map_err(|e| anyhow::anyhow!("bad output: {e}"))?;
        match app
            .mixer
            .request(|ack| Command::AddOutput(Box::new(cfg), Some(ack)))
            .await
        {
            Ok(()) => return Ok(id),
            Err(e) if e.to_string().contains("already exists") => continue,
            Err(e) => return Err(e),
        }
    }
    anyhow::bail!("could not find a free id for {base_id}")
}

/// The id itself, then a few numbered variants of it.
fn id_candidates(base_id: &str) -> impl Iterator<Item = String> + '_ {
    std::iter::once(base_id.to_string()).chain((2..10).map(move |n| format!("{base_id}-{n}")))
}

/// The id a URL gets when nobody chose one: its host, as a slug.
fn derived_id(uri: &str) -> String {
    slug(&host_of(uri))
}

/// What a "Go Live" button sends. One request, from the customer's backend
/// so the page itself never holds the mixer's token, and the page is on air
/// as soon as it renders.
#[derive(Debug, Deserialize)]
struct GoLiveRequest {
    /// The page to put on air. Plain http(s); `web+` is added here.
    url: String,
    /// Where to send the programme. Added as an output unless one already
    /// sends there. Omit to keep the outputs as they are.
    #[serde(default)]
    rtmp: Option<String>,
    /// "auto" (the default) or "off". See `AddSourceRequest::superimpose`.
    #[serde(default)]
    superimpose: Option<String>,
    /// Source id. Derived from the host when omitted.
    #[serde(default)]
    id: Option<String>,
}

/// How long golive waits for the page to produce a frame before giving up on
/// the take. A page that has not rendered in a minute is not about to.
const GOLIVE_WAIT: Duration = Duration::from_secs(60);

async fn golive(
    State(app): State<AppState>,
    Json(req): Json<GoLiveRequest>,
) -> Result<Response, ApiError> {
    let url = req.url.trim();
    if url.is_empty() {
        return Err(anyhow::anyhow!("golive needs a url").into());
    }
    let uri = crate::input::as_web_uri(url);
    let superimpose = match req
        .superimpose
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        None | Some("") | Some("auto") => "auto",
        Some("off") => "off",
        Some(other) => {
            return Err(anyhow::anyhow!("superimpose must be \"auto\" or \"off\", not {other:?}").into())
        }
    };
    let wanted_id = req.id.map(|i| i.trim().to_string()).filter(|i| !i.is_empty());

    // Reuse before adding. The same page already a source, under the id asked
    // for or under any id when none was, is the same page; adding it again
    // would start a second browser for nothing. The status masks URLs, so the
    // real configs are what gets compared.
    let configs = app.mixer.configs().await?;
    let existing = configs
        .sources
        .iter()
        .find(|s| s.uri == uri && wanted_id.as_deref().is_none_or(|w| w == s.id));
    let source = match existing {
        Some(s) => s.id.clone(),
        None => {
            if let Some(w) = &wanted_id {
                if configs.sources.iter().any(|s| &s.id == w) {
                    return Err(anyhow::anyhow!("source {w} already exists with a different URL").into());
                }
            }
            let base_id = wanted_id.unwrap_or_else(|| derived_id(&uri));
            create_source(&app, base_id, uri, None, superimpose).await?
        }
    };

    let output = match req.rtmp.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        None => None,
        Some(rtmp) => match configs.outputs.iter().find(|o| o.uri == rtmp) {
            Some(o) => Some(o.id.clone()),
            None => Some(create_output(&app, rtmp).await?),
        },
    };

    // A reused source may be live already, in which case the take lands on
    // the poller's first tick and the caller should not be told to wait.
    let state = app
        .mixer
        .status()
        .await?
        .sources
        .iter()
        .find(|s| s.id == source)
        .map(|s| s.state)
        .unwrap_or(SourceState::Connecting);
    take_when_live(app, source.clone());
    info!(%source, output = output.as_deref().unwrap_or("none"), "golive accepted");
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "source": source, "output": output, "state": state })),
    )
        .into_response())
}

/// Take the source to programme the moment it is live. Runs on its own
/// because a page can take a while to load and the caller has its answer
/// already. A source missing from the status is treated as not yet there
/// rather than gone, since a superimposed page drops out briefly while it is
/// rebuilt; the deadline covers the case where it really was removed.
fn take_when_live(app: AppState, id: String) {
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + GOLIVE_WAIT;
        let mut ticks = tokio::time::interval(Duration::from_millis(250));
        loop {
            ticks.tick().await;
            // A failed status means the mixer is gone; nothing left to take on.
            let Ok(status) = app.mixer.status().await else { return };
            let live = status.sources.iter().any(|s| s.id == id && s.state == SourceState::Live);
            if live {
                let source = id.clone();
                let r = app
                    .mixer
                    .request(|ack| Command::Take {
                        source: Some(source),
                        at_running_time_ms: None,
                        ack: Some(ack),
                    })
                    .await;
                match r {
                    Ok(()) => info!(source = %id, "golive: on programme"),
                    Err(e) => warn!(source = %id, error = %e, "golive: take failed"),
                }
                return;
            }
            if tokio::time::Instant::now() >= deadline {
                warn!(
                    source = %id,
                    waited_secs = GOLIVE_WAIT.as_secs(),
                    "golive: source never went live, leaving the programme as it is"
                );
                return;
            }
        }
    });
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

/// What a fader sends. Every part is optional, because the UI moves one
/// control at a time and has no reason to restate the others.
#[derive(Debug, Deserialize)]
struct AudioRequest {
    /// The operator's fader for the whole source, 0.0 to 10.0. Works on any
    /// source. Omit to leave it where it is.
    #[serde(default)]
    gain: Option<f64>,
    /// Mute the whole source. Works on any source, and is held apart from the
    /// fader so unmuting comes back to the level that was set.
    #[serde(default)]
    muted: Option<bool>,
    /// Gain on the page's own sound. Omit to leave it where it is.
    #[serde(default)]
    page: Option<f64>,
    /// Gain per video underneath, by position. A null entry, or a list
    /// shorter than the number of videos, leaves those alone: sending
    /// `[null, 0.0]` silences the second video and touches nothing else.
    #[serde(default)]
    media: Vec<Option<f64>>,
}

/// The loudest a channel can be asked for, matching the ceiling the volume
/// elements apply. Kept here as well so the request is pinned before it
/// reaches the pipeline and the answer cannot disagree with what was sent.
const MAX_GAIN: f64 = 10.0;

/// Pin one gain to the range a volume element accepts.
///
/// Out of range is clamped rather than refused: a fader dragged past the end
/// of its track should still move the sound, and an operator mid-broadcast
/// has better things to do than read a validation error. A value that is not
/// a number at all is refused instead, because `f64::clamp` hands NaN back
/// unchanged and a volume element set to NaN goes silent for good with
/// nothing in the log to say why.
fn checked_gain(gain: f64) -> Result<f64> {
    if gain.is_nan() {
        anyhow::bail!("a gain has to be a number");
    }
    Ok(gain.clamp(0.0, MAX_GAIN))
}

/// Move a source's audio: the operator's fader and mute, and for a superimposed
/// source the balance between its page sound and the videos under it.
///
/// 404 and 409 rather than one failure, because they mean different things to
/// whoever is calling: a wrong id, against a source that exists but has its
/// audio pre-mixed by Chromium and so has nothing to balance. Answering the
/// second with a quiet 200 would leave a caller moving a fader that was never
/// connected to anything. Only `page` and `media` can earn the 409 though. The
/// fader and the mute are elements the program pipeline owns for every source,
/// so a request naming just those works on a camera as well as on a page.
async fn set_source_audio(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AudioRequest>,
) -> Result<Response, ApiError> {
    let gain = req.gain.map(checked_gain).transpose()?;
    let page = req.page.map(checked_gain).transpose()?;
    // A null holds that channel, so it passes through validation untouched.
    let media = req
        .media
        .into_iter()
        .map(|gain| gain.map(checked_gain).transpose())
        .collect::<Result<Vec<_>>>()?;
    let outcome = app.mixer.set_audio(id.clone(), gain, req.muted, page, media).await?;
    Ok(audio_response(&id, outcome))
}

fn audio_response(id: &str, outcome: AudioOutcome) -> Response {
    match outcome {
        AudioOutcome::Set(state) => (StatusCode::OK, Json(state)).into_response(),
        AudioOutcome::NoSuchSource => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("no such source {id}") })),
        )
            .into_response(),
        AudioOutcome::NotSuperimposed => (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!(
                    "source {id} is not superimposed, so its sounds arrive already mixed \
                     and there is nothing to balance; its gain and mute still work"
                )
            })),
        )
            .into_response(),
    }
}

/// Where to move a source to, in milliseconds from its start.
#[derive(Debug, Deserialize)]
struct SeekRequest {
    /// Signed and fractional on purpose, so that anything a scrubber can
    /// plausibly send is clamped rather than refused. See `checked_position`.
    position_ms: f64,
}

/// Pin a requested position to something a pipeline can be asked for.
///
/// Out of range is clamped rather than refused, the same bargain `checked_gain`
/// makes: a scrubber dragged past either end of its track should land at that
/// end, and an operator mid-broadcast should not be reading a validation error.
/// The far end is clamped by the mixer instead, which is the only thing that
/// knows the duration. A value that is not a number at all is refused, because
/// there is no sensible position to take it as.
fn checked_position(position_ms: f64) -> Result<u64> {
    if position_ms.is_nan() {
        anyhow::bail!("a position has to be a number");
    }
    // `as` on a float saturates in Rust, so a silly number becomes the ceiling
    // rather than wrapping to somewhere near the start.
    Ok(position_ms.max(0.0).round() as u64)
}

/// Move a seekable source to a position and answer with where it landed.
///
/// 404, 409 and 200 mean three different things to whoever is calling. A wrong
/// id is a bug in the caller. A live feed is a source that exists and is working
/// perfectly and simply has no position to move to, which is worth saying rather
/// than answering 200 and leaving a scrubber to drift back on the next status
/// snapshot. And the 200 carries the position read back off the pipeline, which
/// is not quite the one that was asked for, because a seek snaps to a key unit.
async fn seek_source(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SeekRequest>,
) -> Result<Response, ApiError> {
    let position_ms = checked_position(req.position_ms)?;
    let outcome = app.mixer.seek(id.clone(), position_ms).await?;
    Ok(seek_response(&id, outcome))
}

fn seek_response(id: &str, outcome: SeekOutcome) -> Response {
    match outcome {
        SeekOutcome::Moved(at) => (StatusCode::OK, Json(at)).into_response(),
        SeekOutcome::NoSuchSource => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("no such source {id}") })),
        )
            .into_response(),
        SeekOutcome::NotSeekable => (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!(
                    "source {id} cannot be scrubbed: a live feed has no position to move \
                     to, it is wherever it is now"
                )
            })),
        )
            .into_response(),
        // A pipeline that took the request and refused it is an ordinary failure,
        // so it answers 400 like every other one, carrying the reason in the same
        // `error` field as the two above. The UI shows it verbatim.
        SeekOutcome::Failed(message) => {
            warn!(source = %id, %message, "seek refused");
            (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
        }
    }
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

/// The compact document an agent reads instead of `/api/status`. See
/// `snapshot::AgentState` for what is in it and why.
async fn agent_state(
    State(app): State<AppState>,
    State(snapshots): State<Arc<Tracker>>,
) -> Result<Response, ApiError> {
    // Reading the state is what asks for motion. An agent polling this keeps
    // the tracker alive; one that stops gets the CPU back a few seconds later.
    snapshots.want();
    let status = app.mixer.status().await?;
    let doc = snapshot::agent_state(&status, snapshots.latest().as_ref(), snapshots.enabled());
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(doc)).into_response())
}

#[derive(Debug, Deserialize)]
struct SnapshotQuery {
    /// Downscale to this many pixels across, keeping the aspect. Never
    /// enlarges. Omit for the `[snapshot] default_width` of 320; `width=0`
    /// for the cell's own size.
    #[serde(default)]
    width: Option<u32>,
    /// Ignore the per client rate limit for this one request.
    #[serde(default)]
    force: bool,
    /// Permit a width above `[snapshot] max_width`.
    #[serde(default)]
    allow_large: bool,
}

/// Who is asking, for the snapshot rate limit. The peer address when the
/// server was started with connect info (always, in production), the bearer
/// token when there is one and no address, and otherwise one shared bucket.
fn client_key(req: &Request) -> String {
    if let Some(peer) = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
    {
        return peer.0.ip().to_string();
    }
    match req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        Some(token) => format!("token:{token}"),
        None => "anonymous".into(),
    }
}

/// `sheet.jpg`, `program.jpg` or `{source_id}.jpg`: the newest mosaic frame,
/// or one cell cut out of it. Cutting means a decode and an encode, which is
/// a few milliseconds of CPU and goes on a blocking thread.
///
/// Errors are plain text with a status an agent can branch on: 404 for a
/// name that is not on the mosaic or a mosaic that is switched off, 503 while
/// the first frame is still on its way.
async fn snapshot_image(
    State(snapshots): State<Arc<Tracker>>,
    Path(name): Path<String>,
    Query(q): Query<SnapshotQuery>,
    req: Request,
) -> Response {
    let plain = |code: StatusCode, msg: String| {
        (code, [(header::CACHE_CONTROL, "no-store")], msg).into_response()
    };

    let Some(pick) = snapshot::parse_pick(&name) else {
        return plain(
            StatusCode::NOT_FOUND,
            "no such snapshot; use sheet.jpg, program.jpg or {source_id}.jpg".into(),
        );
    };
    if let Some(why) = snapshots.disabled_reason() {
        return plain(StatusCode::NOT_FOUND, why);
    }
    let ask = Ask { width: q.width, force: q.force, allow_large: q.allow_large };
    let width = match snapshots.resolve(&client_key(&req), &ask) {
        Ok(w) => w,
        Err(refusal @ snapshot::Refusal::TooWide { .. }) => {
            return plain(StatusCode::BAD_REQUEST, refusal.message())
        }
        Err(refusal) => return plain(StatusCode::TOO_MANY_REQUESTS, refusal.message()),
    };
    // Asking is what starts the tracker and, through it, the mosaic. The first
    // request after a quiet spell pays for the build; the rest are free.
    let Some(latest) = snapshots.latest_wanted(Duration::from_secs(3)).await else {
        return plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "no mosaic frame yet: the mosaic is being built for you. Retry in a second."
                .into(),
        );
    };

    // The whole sheet at its own size is the frame as it came off the
    // encoder, no work at all.
    if pick == Pick::Sheet && width.is_none() {
        return jpeg_response(latest.jpeg.to_vec());
    }
    let cell = match &pick {
        Pick::Sheet => None,
        Pick::Program => match snapshot::find_cell(&latest.cells, &pick) {
            Some(c) => Some(c.clone()),
            None => return plain(StatusCode::NOT_FOUND, "the programme is not on the mosaic".into()),
        },
        Pick::Source(id) => match snapshot::find_cell(&latest.cells, &pick) {
            Some(c) => Some(c.clone()),
            None => return plain(StatusCode::NOT_FOUND, format!("no source {id} on the mosaic")),
        },
    };

    let bytes = latest.jpeg.clone();
    let encoded = tokio::task::spawn_blocking(move || {
        let mosaic = snapshot::decode_jpeg(&bytes)?;
        let img = match &cell {
            Some(c) => snapshot::crop_cell(&mosaic, c),
            None => mosaic,
        };
        snapshot::encode_jpeg(&snapshot::fit_width(img, width))
    })
    .await;
    match encoded {
        Ok(Ok(jpeg)) => jpeg_response(jpeg),
        Ok(Err(e)) => {
            warn!(error = %e, "snapshot re-encode failed");
            plain(StatusCode::INTERNAL_SERVER_ERROR, "the mosaic frame could not be decoded".into())
        }
        Err(e) => {
            warn!(error = %e, "snapshot task failed");
            plain(StatusCode::INTERNAL_SERVER_ERROR, "snapshot task failed".into())
        }
    }
}

fn jpeg_response(bytes: Vec<u8>) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "no-store")],
        bytes,
    )
        .into_response()
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
    // Holding this is what keeps the mosaic up. It goes when the socket does,
    // and with it, after the linger, the mosaic itself if this was the last
    // client. A UI that asks for nothing in particular gets the configured
    // size.
    let mut frames = app.multiview.subscribe(MultiviewRequest::configured());

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

            // With multiview off this never yields, so the select just
            // handles events and no special case is needed.
            frame = frames.recv() => match frame {
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
    let snapshots =
        Tracker::new(state.snapshot.clone(), state.multiview.clone(), state.mixer.clone());
    // Connect info so the snapshot rate limit can tell one client from
    // another. Nothing else uses it, and a request without it still works.
    let app = router(state, snapshots)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();
    axum::serve(listener, app).await?;
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

    /// golive names a source and an output after their host. The web+ prefix
    /// and the port must not leak into the id, and a bare host given to
    /// `as_web_uri` must come out the same as its https form.
    #[test]
    fn golive_ids_come_from_the_host() {
        assert_eq!(derived_id("web+http://127.0.0.1:8090/demo.html"), "127-0-0-1");
        assert_eq!(derived_id("web+https://www.example.com/live?x=1"), "example-com");
        assert_eq!(derived_id(&crate::input::as_web_uri("example.com/page")), "example-com");
        assert_eq!(derived_id("rtmp://a.rtmp.youtube.com/live2/KEY"), "a-rtmp-youtube-com");
        assert_eq!(derived_id("web+"), "source");
        let mut c = id_candidates("demo");
        assert_eq!(c.next().as_deref(), Some("demo"));
        assert_eq!(c.next().as_deref(), Some("demo-2"));
        assert_eq!(c.last().as_deref(), Some("demo-9"));
    }

    fn headers_with(auth: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(a) = auth {
            h.insert(header::AUTHORIZATION, a.parse().unwrap());
        }
        h
    }

    fn uri(s: &str) -> Uri {
        s.parse().unwrap()
    }

    #[test]
    fn no_token_configured_means_everything_is_open() {
        let plain = uri("/api/status");
        assert_eq!(token_check(None, &Method::GET, &headers_with(None), &plain), Ok(()));
        assert_eq!(token_check(None, &Method::POST, &headers_with(Some("Bearer junk")), &plain), Ok(()));
    }

    #[test]
    fn token_in_the_header_is_checked_on_every_method() {
        let plain = uri("/api/take");
        let ok = headers_with(Some("Bearer s3cret"));
        assert_eq!(token_check(Some("s3cret"), &Method::POST, &ok, &plain), Ok(()));
        assert_eq!(token_check(Some("s3cret"), &Method::GET, &ok, &plain), Ok(()));
        assert_eq!(token_check(Some("s3cret"), &Method::DELETE, &ok, &plain), Ok(()));
        // The scheme is case insensitive, as HTTP says it is.
        let lower = headers_with(Some("bearer s3cret"));
        assert_eq!(token_check(Some("s3cret"), &Method::POST, &lower, &plain), Ok(()));

        assert_eq!(
            token_check(Some("s3cret"), &Method::POST, &headers_with(None), &plain),
            Err("missing token")
        );
        let wrong = headers_with(Some("Bearer s3cres"));
        assert_eq!(token_check(Some("s3cret"), &Method::POST, &wrong, &plain), Err("wrong token"));
        let short = headers_with(Some("Bearer s3cre"));
        assert_eq!(token_check(Some("s3cret"), &Method::POST, &short, &plain), Err("wrong token"));
        let long = headers_with(Some("Bearer s3cret1"));
        assert_eq!(token_check(Some("s3cret"), &Method::POST, &long, &plain), Err("wrong token"));
        // Another scheme is not a bearer token at all.
        let basic = headers_with(Some("Basic czNjcmV0"));
        assert_eq!(token_check(Some("s3cret"), &Method::GET, &basic, &plain), Err("missing token"));
    }

    #[test]
    fn token_in_the_query_is_taken_on_get_only() {
        let none = headers_with(None);
        let right = uri("/ws?token=s3cret");
        assert_eq!(token_check(Some("s3cret"), &Method::GET, &none, &right), Ok(()));
        // Percent encoded, as a browser would send it, and among other keys.
        let encoded = uri("/ws?x=1&token=s3%63ret&y=2");
        assert_eq!(token_check(Some("s3cret"), &Method::GET, &none, &encoded), Ok(()));
        let wrong = uri("/ws?token=nope");
        assert_eq!(token_check(Some("s3cret"), &Method::GET, &none, &wrong), Err("wrong token"));
        let empty = uri("/ws?token=");
        assert_eq!(token_check(Some("s3cret"), &Method::GET, &none, &empty), Err("missing token"));
        // A POST does not get to put the token in its URL.
        assert_eq!(token_check(Some("s3cret"), &Method::POST, &none, &right), Err("missing token"));
        // A header wins over a query when both are present, wrong or not.
        let bad_header = headers_with(Some("Bearer nope"));
        assert_eq!(token_check(Some("s3cret"), &Method::GET, &bad_header, &right), Err("wrong token"));
    }

    /// A slider that overshoots still moves the sound, so out of range is
    /// clamped. NaN is the one value refused: it cannot arrive from JSON, but
    /// `f64::clamp` would hand it straight through to a volume element that
    /// then goes silent with nothing in the log to explain it.
    #[test]
    fn gains_are_clamped_before_they_reach_the_pipeline() {
        assert_eq!(checked_gain(0.5).unwrap(), 0.5);
        assert_eq!(checked_gain(-2.0).unwrap(), 0.0);
        assert_eq!(checked_gain(1e6).unwrap(), MAX_GAIN);
        assert_eq!(checked_gain(f64::INFINITY).unwrap(), MAX_GAIN);
        assert_eq!(checked_gain(f64::NEG_INFINITY).unwrap(), 0.0);
        assert!(checked_gain(f64::NAN).is_err());
    }

    /// Every field optional, and a missing `media` is an empty list rather
    /// than a list of zeroes: the difference is whether sending one fader
    /// silences every video underneath it.
    #[test]
    fn a_partial_audio_body_names_only_what_it_moves() {
        let parse = |v: serde_json::Value| serde_json::from_value::<AudioRequest>(v).unwrap();
        let page_only = parse(serde_json::json!({ "page": 0.8 }));
        assert_eq!(page_only.page, Some(0.8));
        assert!(page_only.media.is_empty());
        assert_eq!(page_only.gain, None);
        assert_eq!(page_only.muted, None);

        // What a source's own fader sends, and what the mute button sends.
        // Neither names a balance, which is what keeps them off the 409 path.
        let fader = parse(serde_json::json!({ "gain": 0.4 }));
        assert_eq!(fader.gain, Some(0.4));
        assert_eq!(fader.muted, None);
        assert_eq!(fader.page, None);
        assert!(fader.media.is_empty());

        let mute = parse(serde_json::json!({ "muted": true }));
        assert_eq!(mute.muted, Some(true));
        assert_eq!(mute.gain, None);

        // Unmuting is an explicit false, not an omission. Omitting it has to
        // leave the mute alone, or every fader move would unmute the source.
        let unmute = parse(serde_json::json!({ "muted": false }));
        assert_eq!(unmute.muted, Some(false));

        let media_only = parse(serde_json::json!({ "media": [1.0, 0.0] }));
        assert_eq!(media_only.page, None);
        assert_eq!(media_only.media, vec![Some(1.0), Some(0.0)]);

        // An empty body is a read of the current levels, not a reset.
        let nothing = parse(serde_json::json!({}));
        assert_eq!(nothing.page, None);
        assert!(nothing.media.is_empty());
        assert_eq!(nothing.gain, None);
        assert_eq!(nothing.muted, None);

        let both = parse(serde_json::json!({ "page": 0.8, "media": [1.0, 0.0] }));
        assert_eq!(both.page, Some(0.8));
        assert_eq!(both.media, vec![Some(1.0), Some(0.0)]);

        // What the UI actually sends when the second video's fader moves. A
        // short list could not say this: `[0.5]` would move the first video.
        let second_only = parse(serde_json::json!({ "media": [null, 0.5] }));
        assert_eq!(second_only.page, None);
        assert_eq!(second_only.media, vec![None, Some(0.5)]);
    }

    async fn body_json(r: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Three answers, three statuses. The 409 is the one that earns its keep:
    /// a whole page source exists and takes the request happily, but its
    /// sounds were mixed by Chromium and there is nothing behind the balance
    /// faders. A quiet 200 there would have the caller dragging a dead control.
    #[tokio::test]
    async fn balancing_says_which_kind_of_no_it_is() {
        let ok = audio_response(
            "page",
            AudioOutcome::Set(crate::state::SourceAudioState {
                gain: 1.0,
                muted: false,
                page: Some(0.8),
                media: Some(vec![1.0, 0.0]),
            }),
        );
        assert_eq!(ok.status(), StatusCode::OK);
        let v = body_json(ok).await;
        assert_eq!(v["gain"], 1.0);
        assert_eq!(v["muted"], false);
        assert_eq!(v["page"], 0.8);
        assert_eq!(v["media"][1], 0.0);

        // A camera answers with the fader and the mute and nothing else. The UI
        // draws balance faders when `page` is there, so writing a null would
        // have it drawing controls with nothing behind them.
        let camera = audio_response(
            "cam1",
            AudioOutcome::Set(crate::state::SourceAudioState {
                gain: 0.4,
                muted: true,
                page: None,
                media: None,
            }),
        );
        assert_eq!(camera.status(), StatusCode::OK);
        let v = body_json(camera).await;
        assert_eq!(v["gain"], 0.4);
        assert_eq!(v["muted"], true);
        assert!(v.get("page").is_none(), "a camera answer must carry no balance");
        assert!(v.get("media").is_none(), "a camera answer must carry no balance");

        let missing = audio_response("cam9", AudioOutcome::NoSuchSource);
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let v = body_json(missing).await;
        assert!(
            v["error"].as_str().unwrap().contains("cam9"),
            "the message has to name the id that was asked for: {v}"
        );

        let flat = audio_response("cam1", AudioOutcome::NotSuperimposed);
        assert_eq!(flat.status(), StatusCode::CONFLICT);
        let v = body_json(flat).await;
        let msg = v["error"].as_str().unwrap();
        assert!(msg.contains("cam1") && msg.contains("superimposed"), "unclear message: {msg}");
    }

    /// The scrubber's three answers. The 409 is the one worth having: a camera
    /// exists, works, and has no position to move to, and a quiet 200 there
    /// would leave the UI drawing a scrubber that snaps back on the next poll.
    #[tokio::test]
    async fn seeking_says_which_kind_of_no_it_is() {
        let ok = seek_response(
            "clip1",
            SeekOutcome::Moved(crate::state::SourcePositionState {
                position_ms: 42_000,
                duration_ms: Some(154_000),
            }),
        );
        assert_eq!(ok.status(), StatusCode::OK);
        let v = body_json(ok).await;
        assert_eq!(v["position_ms"], 42_000);
        assert_eq!(v["duration_ms"], 154_000);

        // A clip whose duration the demuxer has not worked out yet still says
        // where it landed, and leaves the duration out rather than writing a zero
        // that the UI would draw a full length track from.
        let early = seek_response(
            "clip1",
            SeekOutcome::Moved(crate::state::SourcePositionState {
                position_ms: 1_000,
                duration_ms: None,
            }),
        );
        let v = body_json(early).await;
        assert_eq!(v["position_ms"], 1_000);
        assert!(v.get("duration_ms").is_none());

        let missing = seek_response("cam9", SeekOutcome::NoSuchSource);
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let v = body_json(missing).await;
        assert!(
            v["error"].as_str().unwrap().contains("cam9"),
            "the message has to name the id that was asked for: {v}"
        );

        let live = seek_response("cam1", SeekOutcome::NotSeekable);
        assert_eq!(live.status(), StatusCode::CONFLICT);
        let v = body_json(live).await;
        let msg = v["error"].as_str().unwrap();
        assert!(msg.contains("cam1") && msg.contains("live feed"), "unclear message: {msg}");

        // A pipeline that took the request and refused it is an ordinary failure
        // and comes back as one, with the reason intact.
        let refused =
            seek_response("clip1", SeekOutcome::Failed("demuxer refused the seek".into()));
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
        let v = body_json(refused).await;
        assert!(v["error"].as_str().unwrap().contains("demuxer refused"), "{v}");
    }

    /// A scrubber dragged off either end of its track should land at that end.
    /// The near end is clamped here; the far one is clamped by the mixer, which
    /// is the only thing that knows how long the clip is.
    #[test]
    fn a_position_off_the_end_of_the_track_is_clamped_not_refused() {
        assert_eq!(checked_position(0.0).unwrap(), 0);
        assert_eq!(checked_position(42_000.0).unwrap(), 42_000);
        // Dragged past the left hand end, which a scrubber does on a quick flick.
        assert_eq!(checked_position(-5_000.0).unwrap(), 0);
        // Fractions come of dividing a pixel position by a track width.
        assert_eq!(checked_position(41_999.6).unwrap(), 42_000);
        // Saturating rather than wrapping: a silly number must not land near the
        // start of the clip, which is what `as` on a float used to do.
        assert_eq!(checked_position(1e300).unwrap(), u64::MAX);
        // Not a number is the one thing refused, because there is no position to
        // read it as.
        assert!(checked_position(f64::NAN).is_err());
    }

    #[test]
    fn the_seek_request_needs_a_position() {
        let parse = |v: serde_json::Value| serde_json::from_value::<SeekRequest>(v);
        assert_eq!(parse(serde_json::json!({ "position_ms": 42000 })).unwrap().position_ms, 42_000.0);
        // An integer and a float both arrive as the same thing, so the UI can
        // send whatever its slider gives it.
        assert_eq!(parse(serde_json::json!({ "position_ms": 42000.5 })).unwrap().position_ms, 42_000.5);
        // An empty body is not a read here. There is nothing to read: the
        // position is in the status snapshot and in the position event already.
        assert!(parse(serde_json::json!({})).is_err());
    }

    #[test]
    fn constant_time_eq_agrees_with_plain_equality() {
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"ab", b"abc"));
        assert!(!constant_time_eq(b"abc", b""));
    }
}
