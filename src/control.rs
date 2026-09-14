//! HTTP and WebSocket control plane.
//!
//! The UI is a plain web app served by this binary, which is what makes the
//! Tauri desktop app and remote browser control the same program. Tauri points
//! a webview at this server; a remote operator points a browser at it. There
//! is no second implementation to keep in step.
//!
//! Three doors onto one set of methods:
//!
//! * `/rpc`, a WebSocket carrying JSON-RPC 2.0 text frames and mosaic frames
//!   as binary. This is what a UI, a node or a service uses.
//! * `/api/v1`, generated from the method names by the transform rule in 03
//!   section 6, for curl, an `<img>` tag and anything with an HTTP client.
//! * `/api` and `/ws`, the routes that existed before, kept working for one
//!   release and answered with a `Deprecation` header.
//!
//! The methods themselves are in `control/methods.rs` and everything that
//! happens around a call is in `control/call.rs`. This file is the plumbing:
//! what the server holds, how a request finds a method, and how the two
//! background tasks that feed the event stream are started.

pub mod call;
pub mod history;
pub mod methods;
pub mod rest;
pub mod ws;

use crate::api::error::{ErrorCode, RpcError};
use crate::api::idempotency;
use crate::api::method::Registry;
use crate::api::scope::{Confirmations, Tokens};
use crate::api::types::{CanvasInfo, Limits, MultiviewStatus};
use crate::api::{AddSourceRequest, GoLiveRequest, GoLiveResult, MultiviewLayout};
use crate::config::{Config, OutputConfig, SnapshotConfig, SourceConfig};
use crate::media::{MediaLibrary, MediaListing};
use crate::mixer::{AudioOutcome, Command, MixerHandle, SeekOutcome};
use crate::multiview::MultiviewHandle;
use crate::snapshot::{self, Ask, Pick, Tracker};
use crate::state::{Event, MixerStatus, SourceState};
use anyhow::Result;
use axum::body::Body;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{DefaultBodyLimit, FromRef, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use call::Call;
use history::History;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

/// What every legacy route answers with, so a client watching its own logs
/// finds out before the routes go rather than after.
const SUNSET_NOTE: &str = "/api and /ws are replaced by /api/v1 and /rpc and will be removed \
                           one release after this one. See GET /api/v1/core/api.";

#[derive(Clone)]
pub struct AppState {
    pub mixer: MixerHandle,
    /// The mosaic. Subscribing through this is what builds it; there is no
    /// other way to reach the frames, and holding a subscription is the only
    /// thing that keeps the pipeline up. See `multiview.rs`.
    pub multiview: MultiviewHandle,
    /// The still and motion limits, `[snapshot]` in the config.
    pub snapshot: SnapshotConfig,
    /// Ad clips available on this machine.
    pub library: Arc<MediaLibrary>,
    /// Runs file transcodes and remembers their progress.
    pub converter: Arc<crate::convert::Converter>,
    /// Rung by `core.shutdown`. `main` waits on it alongside Ctrl-C and takes
    /// the whole process down the same way for either.
    pub quit: Arc<tokio::sync::Notify>,
    /// Every credential this core accepts. Empty leaves the control port
    /// open, which is the original behaviour and right for a mixer that only
    /// listens on a LAN nobody else is on.
    pub tokens: Arc<Tokens>,
    pub confirmations: Arc<Confirmations>,
    pub idempotency: Arc<idempotency::Cache>,
    pub history: Arc<History>,
    pub features: Arc<Vec<String>>,
    pub limits: Limits,
    pub canvas: CanvasInfo,
    /// True when the core was started with `--rehearsal`.
    pub rehearsal: bool,
}

impl AppState {
    /// Everything the control plane holds, worked out from the config once.
    pub fn new(
        cfg: &Config,
        mixer: MixerHandle,
        multiview: MultiviewHandle,
        library: Arc<MediaLibrary>,
        converter: Arc<crate::convert::Converter>,
        quit: Arc<tokio::sync::Notify>,
        rehearsal: bool,
    ) -> Self {
        let tokens = cfg.tokens(rehearsal);
        Self {
            mixer,
            multiview,
            snapshot: cfg.snapshot.clone(),
            library,
            converter,
            quit,
            features: Arc::new(features(cfg, &tokens, rehearsal)),
            limits: Limits {
                max_upload_bytes: cfg.media.max_upload_bytes,
                max_gain: MAX_GAIN,
                max_call_secs: crate::api::MAX_CALL_SECS,
                max_idempotency_key_bytes: 255,
                event_queue: 256,
            },
            canvas: CanvasInfo {
                width: cfg.canvas.width,
                height: cfg.canvas.height,
                fps: cfg.canvas.fps,
            },
            tokens: Arc::new(tokens),
            confirmations: Confirmations::new(),
            idempotency: idempotency::Cache::new(),
            history: Arc::new(History::new()),
            rehearsal,
        }
    }
}

/// What `core.info` reports, so a client branches on a feature string rather
/// than on a 404 it has to provoke first.
fn features(cfg: &Config, tokens: &Tokens, rehearsal: bool) -> Vec<String> {
    let mut features = vec!["api-v1".to_string(), "mcp".to_string(), "rpc".to_string()];
    if cfg.multiview.enabled {
        features.push("multiview".into());
        // Snapshots are cut out of the mosaic, so there are none without it.
        features.push("snapshot".into());
    }
    if cfg.media.allow_upload {
        features.push("uploads".into());
    }
    if cfg.security.allow_exec_sources {
        features.push("exec-sources".into());
    }
    if !tokens.is_open() {
        features.push("tokens".into());
    }
    if rehearsal {
        features.push("rehearsal".into());
    }
    features.sort();
    features
}

/// What the router carries: the state, the snapshot tracker, and the method
/// table with the routes generated from it.
#[derive(Clone)]
pub struct Ctx {
    pub app: AppState,
    pub snapshots: Arc<Tracker>,
    pub registry: Arc<Registry<Call>>,
    pub routes: Arc<Vec<rest::Route>>,
    /// The deprecated paths, resolvable the same way, so the token check in
    /// front of them can apply the scope of the method each one aliases.
    pub legacy_routes: Arc<Vec<rest::Route>>,
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
    let registry = Arc::new(methods::registry());
    let routes = Arc::new(rest::routes(registry.as_ref()));
    let legacy_routes = Arc::new(rest::legacy_routes());
    let ctx = Ctx { app, snapshots, registry, routes, legacy_routes };

    // The page, its modules, its themes and `/plugins/<name>/ui/` are open:
    // they are the same for everyone, hold nothing secret, and are where a
    // browser finds out that it needs a token at all.
    Router::new()
        .merge(crate::ui::router())
        .route("/rpc", get(rpc_upgrade))
        .merge(legacy(ctx.clone(), max_upload))
        .merge(rest::router(ctx.clone(), max_upload))
        // The Tauri shell and a browser on another origin both need this. It
        // sits outside the token check so that a preflight, which carries no
        // Authorization header by design, is answered rather than refused.
        .layer(CorsLayer::permissive())
        .with_state(ctx)
}

/// The routes that existed before `/api/v1`, unchanged in shape and answered
/// with a `Deprecation` header. The web UI and the Python example use them.
fn legacy(ctx: Ctx, max_upload: usize) -> Router<Ctx> {
    Router::new()
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
        .route_layer(middleware::from_fn(mark_deprecated))
        .route_layer(middleware::from_fn_with_state(ctx.clone(), guard_legacy))
        .with_state(ctx)
}

/// Say so on the way out, on every legacy answer.
async fn mark_deprecated(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert("deprecation", HeaderValue::from_static("true"));
    if let Ok(note) = HeaderValue::from_str(SUNSET_NOTE) {
        headers.insert("link", note);
    }
    response
}

/// The token check in front of the deprecated routes.
///
/// It applies the scope of the method each path aliases, because two doors
/// onto one set of methods must not mean two sets of permissions: a read only
/// token that could still `POST /api/take` would make the whole table a
/// decoration. The refusal keeps the plain `{"error": ...}` shape those
/// clients already parse.
async fn guard_legacy(State(ctx): State<Ctx>, req: Request, next: Next) -> Response {
    let presented = presented_token(req.method(), req.headers(), req.uri());
    let token = match ctx.app.tokens.authenticate(presented.as_deref()) {
        Ok(t) => t,
        Err(reason) => {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer")],
                Json(json!({ "error": reason.message() })),
            )
                .into_response()
        }
    };
    if let Some(refusal) = legacy_refusal(&ctx, &token, req.method(), req.uri().path()) {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": refusal }))).into_response();
    }
    next.run(req).await
}

/// Why this token may not use this legacy path, if it may not.
fn legacy_refusal(
    ctx: &Ctx,
    token: &crate::api::scope::Token,
    http: &Method,
    path: &str,
) -> Option<String> {
    let (route, _) = rest::resolve(&ctx.legacy_routes, http, path).ok()?;
    let def = ctx.registry.get(route.method)?;
    if !token.has(def.scope) {
        return Some(
            RpcError::scope(route.method, def.scope.as_str(), &token.scope_names()).message,
        );
    }
    if ctx.app.rehearsal && route.method == "output.add" {
        return Some(
            "this core was started with --rehearsal and will not add an output, so nothing \
             here reaches a real destination. Start a core without --rehearsal to go on air."
                .to_string(),
        );
    }
    None
}

/// The token a request carries. `Authorization: Bearer <token>` is the normal
/// form. A GET may put it in the query instead, because a browser opening a
/// WebSocket or an `<img>` tag has no way to set a header. Other methods do
/// not get the query form: they come from code that can set the header, and a
/// token in a POST's URL ends up in more logs than it should.
pub fn presented_token(method: &Method, headers: &HeaderMap, uri: &Uri) -> Option<String> {
    rest::bearer(headers)
        .or_else(|| if method == Method::GET { rest::query_token(uri) } else { None })
}

/// The trace id in force for one HTTP request.
pub fn trace_of(headers: &HeaderMap, explicit: Option<&str>) -> String {
    trace_id_of(headers, explicit).to_string()
}

/// The same id, typed, for `observe::with_trace_id` so every log line a call
/// produces carries it without being passed an argument.
pub fn trace_id_of(headers: &HeaderMap, explicit: Option<&str>) -> crate::observe::TraceId {
    let traceparent =
        headers.get(crate::api::trace::TRACEPARENT).and_then(|v| v.to_str().ok());
    crate::api::trace::incoming(traceparent, explicit)
}

/// The whole protocol document, built once.
///
/// `core.api`, `godwinmix --api-info` and the committed `protocol.json` are
/// all this value. Built from a fresh registry, so it needs no mixer and can
/// be printed on a machine with no GStreamer and no configuration.
pub fn descriptor() -> &'static Value {
    static DOC: OnceLock<Value> = OnceLock::new();
    DOC.get_or_init(|| crate::api::protocol::descriptor(&methods::registry()))
}

/// The OpenAPI 3.1 description of the REST layer, built once.
///
/// The committed `openapi.json`, and what a client generator or Swagger UI
/// reads. Built from the same table as everything else.
pub fn openapi() -> &'static Value {
    static DOC: OnceLock<Value> = OnceLock::new();
    DOC.get_or_init(|| crate::api::openapi::openapi(&methods::registry()))
}

async fn rpc_upgrade(
    ws: WebSocketUpgrade,
    State(ctx): State<Ctx>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let presented = presented_token(&Method::GET, &headers, &uri);
    let token = match ctx.app.tokens.authenticate(presented.as_deref()) {
        Ok(t) => t,
        Err(reason) => {
            let trace = trace_of(&headers, None);
            let e = RpcError::new(
                ErrorCode::Scope,
                format!(
                    "{}. Open /rpc?token=<token>, or send an Authorization header.",
                    reason.message()
                ),
            );
            return (StatusCode::UNAUTHORIZED, Json(e.body(&trace))).into_response();
        }
    };
    ws.on_upgrade(move |socket| ws::serve_rpc(socket, ctx, token))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(ctx): State<Ctx>) -> Response {
    ws.on_upgrade(move |socket| ws::serve_legacy(socket, ctx))
}

// --- work shared by the legacy handlers and the method table ---------------

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
pub fn checked_gain(gain: f64) -> Result<f64> {
    if gain.is_nan() {
        anyhow::bail!("a gain has to be a number");
    }
    Ok(gain.clamp(0.0, MAX_GAIN))
}

/// Pin a requested position to something a pipeline can be asked for.
///
/// Out of range is clamped rather than refused, the same bargain
/// `checked_gain` makes. The far end is clamped by the mixer instead, which is
/// the only thing that knows the duration.
pub fn checked_position(position_ms: f64) -> Result<u64> {
    if position_ms.is_nan() {
        anyhow::bail!("a position has to be a number");
    }
    // `as` on a float saturates in Rust, so a silly number becomes the ceiling
    // rather than wrapping to somewhere near the start.
    Ok(position_ms.max(0.0).round() as u64)
}

/// Add a source from a request, and say which id it got.
pub async fn add_source_now(app: &AppState, req: AddSourceRequest) -> Result<String> {
    let uri = req.uri.trim().to_string();
    if uri.is_empty() {
        anyhow::bail!("a source needs a URL");
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
    create_source(app, base_id, uri, name, &superimpose, &req.params).await
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
    params: &serde_json::Map<String, Value>,
) -> Result<String> {
    for id in id_candidates(&base_id) {
        // Whatever a source kind of its own understands rides underneath the
        // fields the core knows, so a plugin's keys reach its config and the
        // core's keys still win.
        let mut fields = params.clone();
        fields.insert("id".into(), json!(id));
        fields.insert("uri".into(), json!(uri));
        fields.insert("name".into(), json!(name));
        fields.insert("superimpose".into(), json!(superimpose));
        let cfg: SourceConfig = serde_json::from_value(Value::Object(fields))
            .map_err(|e| anyhow::anyhow!("bad source: {e}"))?;
        match app.mixer.request(|ack| Command::AddSource(Box::new(cfg), Some(ack))).await {
            Ok(()) => return Ok(id),
            Err(e) if e.to_string().contains("already exists") => continue,
            Err(e) => return Err(e),
        }
    }
    anyhow::bail!("could not find a free id for {base_id}")
}

/// Same again for an output, named after the host it sends to.
async fn create_output(app: &AppState, uri: &str) -> Result<String> {
    let base_id = derived_id(uri);
    for id in id_candidates(&base_id) {
        let cfg: OutputConfig = serde_json::from_value(json!({ "id": id, "uri": uri }))
            .map_err(|e| anyhow::anyhow!("bad output: {e}"))?;
        match app.mixer.request(|ack| Command::AddOutput(Box::new(cfg), Some(ack))).await {
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

/// How long golive waits for the page to produce a frame before giving up on
/// the take. A page that has not rendered in a minute is not about to.
const GOLIVE_WAIT: Duration = Duration::from_secs(60);

/// Add the page, add the destination, and arrange for the take.
pub async fn golive_now(app: &AppState, req: GoLiveRequest) -> Result<GoLiveResult> {
    let url = req.url.trim();
    if url.is_empty() {
        anyhow::bail!("golive needs a url");
    }
    let uri = crate::input::as_web_uri(url);
    let superimpose = match req.superimpose.as_deref().map(str::trim) {
        None | Some("") | Some("auto") => "auto",
        Some("off") => "off",
        Some(other) => anyhow::bail!("superimpose must be \"auto\" or \"off\", not {other:?}"),
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
                    anyhow::bail!("source {w} already exists with a different URL");
                }
            }
            let base_id = wanted_id.unwrap_or_else(|| derived_id(&uri));
            create_source(app, base_id, uri, None, superimpose, &serde_json::Map::new()).await?
        }
    };

    let output = match req.rtmp.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        None => None,
        Some(rtmp) => match configs.outputs.iter().find(|o| o.uri == rtmp) {
            Some(o) => Some(o.id.clone()),
            None => Some(create_output(app, rtmp).await?),
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
    take_when_live(app.clone(), source.clone());
    info!(%source, output = output.as_deref().unwrap_or("none"), "golive accepted");
    Ok(GoLiveResult { source, output, state })
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
                app.history.expect("golive");
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

/// Stream an uploaded file to disk. Never buffered: a large clip must cost a
/// chunk of memory, not its whole size, and the process has a live programme
/// in it. Written under a dotted `.part` name and renamed on success so a half
/// uploaded file never appears in the listing and never gets taken to air.
pub async fn store_upload(app: &AppState, name: &str, body: Body) -> Result<Value, RpcError> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    if !app.library.cfg().allow_upload {
        return Err(RpcError::not_in_state(
            "uploads are disabled on this server. Set `allow_upload = true` under [media] \
             and restart, or put the file in the media directory yourself.",
        ));
    }
    let name = crate::media::safe_upload_name(name)
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
    let dir = app.library.dir().to_path_buf();
    let part = dir.join(format!(".{name}.part"));
    let final_path = dir.join(&name);

    let mut file = tokio::fs::File::create(&part)
        .await
        .map_err(|e| RpcError::internal(format!("creating {}: {e}", part.display())))?;
    let mut stream = body.into_data_stream();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                drop(file);
                let _ = tokio::fs::remove_file(&part).await;
                return Err(RpcError::internal(format!("upload interrupted: {e}")));
            }
        };
        written += chunk.len() as u64;
        if let Err(e) = file.write_all(&chunk).await {
            drop(file);
            let _ = tokio::fs::remove_file(&part).await;
            return Err(RpcError::internal(format!("writing upload: {e}")));
        }
    }
    file.flush().await.ok();
    file.sync_all().await.ok();
    drop(file);
    tokio::fs::rename(&part, &final_path)
        .await
        .map_err(|e| RpcError::internal(format!("finishing upload: {e}")))?;

    info!(%name, bytes = written, "media uploaded");
    app.mixer.emit(Event::MediaChanged { name: name.clone(), conversion: None });
    Ok(json!({
        "name": name,
        "path": final_path.display().to_string(),
        "size_bytes": written,
    }))
}

/// One JPEG: the whole sheet, the programme, or a cell cut out of the mosaic.
///
/// Cutting means a decode and an encode, a few milliseconds of CPU, so it
/// goes on a blocking thread rather than on a Tokio worker.
pub async fn snapshot_bytes(
    snapshots: &Arc<Tracker>,
    client: &str,
    name: &str,
    ask: &Ask,
) -> Result<Vec<u8>, RpcError> {
    // The legacy path spells it `cam1.jpg` and the versioned one spells it
    // `cam1`, because `/api/v1/snapshot/{id}` takes an id like every other
    // route. Both reach the same picture.
    let with_suffix = snapshot_name(name);
    let Some(pick) = snapshot::parse_pick(&with_suffix) else {
        return Err(RpcError::not_found("snapshot", name, &[]).with(
            "valid",
            vec!["sheet".to_string(), "program".to_string(), "<source id>".to_string()],
        ));
    };
    if let Some(why) = snapshots.disabled_reason() {
        return Err(RpcError::not_in_state(why));
    }
    // The width the limits allow for this client, which is also what says no
    // when one client asks too often.
    let width = match snapshots.resolve(client, ask) {
        Ok(w) => w,
        Err(refusal @ snapshot::Refusal::TooWide { .. }) => {
            return Err(RpcError::invalid_params(refusal.message()))
        }
        Err(refusal) => {
            return Err(RpcError::new(ErrorCode::Safety, refusal.message()));
        }
    };
    // Asking is what starts the tracker and, through it, the mosaic. The first
    // request after a quiet spell pays for the build; the rest are free.
    let Some(latest) = snapshots.latest_wanted(Duration::from_secs(3)).await else {
        return Err(RpcError::not_in_state(
            "no mosaic frame yet: the mosaic is being built for you. Retry in a second.",
        ));
    };
    if pick == Pick::Sheet && width.is_none() {
        return Ok(latest.jpeg.to_vec());
    }
    let cell = match &pick {
        Pick::Sheet => None,
        _ => match snapshot::find_cell(&latest.cells, &pick) {
            Some(c) => Some(c.clone()),
            None => {
                let on_sheet: Vec<String> =
                    latest.cells.iter().filter_map(|c| c.source.clone()).collect();
                return Err(RpcError::not_found("cell on the mosaic", name, &on_sheet));
            }
        },
    };
    let bytes = latest.jpeg.clone();
    tokio::task::spawn_blocking(move || {
        let mosaic = snapshot::decode_jpeg(&bytes)?;
        let img = match &cell {
            Some(c) => snapshot::crop_cell(&mosaic, c),
            None => mosaic,
        };
        snapshot::encode_jpeg(&snapshot::fit_width(img, width))
    })
    .await
    .map_err(|e| RpcError::internal(format!("snapshot task failed: {e}")))?
    .map_err(|e| RpcError::internal(format!("the mosaic frame could not be decoded: {e}")))
}

/// `sheet`, `sheet.jpg` and `cam1` all name a picture. The tracker's parser
/// wants the extension, and an id in a path has no business carrying one.
fn snapshot_name(name: &str) -> String {
    match name.strip_suffix(".jpg") {
        Some(_) => name.to_string(),
        None => format!("{name}.jpg"),
    }
}

/// The layout a client matches frames against.
pub fn layout_of(multiview: &MultiviewStatus) -> MultiviewLayout {
    MultiviewLayout {
        id: crate::api::rpc::layout_id(&multiview.cells),
        width: multiview.width,
        height: multiview.height,
        cells: multiview.cells.clone(),
    }
}

// --- legacy handlers, unchanged in shape -----------------------------------

/// Anything that goes wrong on a legacy route becomes a 400 with the message,
/// exactly as it did. `/api/v1` has the one error shape; this door keeps the
/// shape the clients behind it already parse.
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
struct TakeBody {
    /// Omit or null to cut to the slate.
    #[serde(default)]
    source: Option<String>,
    /// Running time to land the cut on. Omit for immediate.
    #[serde(default)]
    at_running_time_ms: Option<u64>,
}

async fn take(
    State(app): State<AppState>,
    Json(req): Json<TakeBody>,
) -> Result<StatusCode, ApiError> {
    app.history.expect("legacy");
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

async fn upload_media(
    State(app): State<AppState>,
    Query(q): Query<UploadQuery>,
    body: Body,
) -> Result<Json<Value>, ApiError> {
    store_upload(&app, &q.name, body)
        .await
        .map(Json)
        .map_err(|e| ApiError(anyhow::anyhow!("{}", e.message)))
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
) -> Result<Json<Value>, ApiError> {
    let path = app.library.resolve(&name)?;
    let target = crate::input::to_uri(&path.display().to_string());
    let configs = app.mixer.configs().await?;
    if let Some(s) = configs.sources.iter().find(|s| crate::input::to_uri(&s.uri) == target) {
        return Err(
            anyhow::anyhow!("{name} is the source \"{}\". Remove the source first.", s.id).into()
        );
    }
    let mut removed = Vec::new();
    for p in [path.clone(), crate::convert::converted_sibling(&path)] {
        if p.exists() && std::fs::remove_file(&p).is_ok() {
            removed.push(p.display().to_string());
        }
    }
    app.mixer.emit(Event::MediaChanged { name, conversion: None });
    Ok(Json(json!({ "removed": removed })))
}

#[derive(Debug, Deserialize)]
struct AdBreakBody {
    /// File path or URI of the ad to play.
    uri: String,
    #[serde(default)]
    at_running_time_ms: Option<u64>,
    #[serde(default)]
    return_to: Option<String>,
}

async fn start_ad_break(
    State(app): State<AppState>,
    Json(req): Json<AdBreakBody>,
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

async fn add_source(
    State(app): State<AppState>,
    Json(req): Json<AddSourceRequest>,
) -> Result<StatusCode, ApiError> {
    add_source_now(&app, req).await?;
    Ok(StatusCode::OK)
}

async fn golive(
    State(app): State<AppState>,
    Json(req): Json<GoLiveRequest>,
) -> Result<Response, ApiError> {
    let result = golive_now(&app, req).await?;
    Ok((StatusCode::ACCEPTED, Json(result)).into_response())
}

async fn remove_source(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    app.mixer.request(|ack| Command::RemoveSource(id, Some(ack))).await?;
    Ok(StatusCode::OK)
}

/// Move a source's audio: the operator's fader and mute, and for a
/// superimposed source the balance between its page sound and the videos
/// under it.
async fn set_source_audio(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::api::AudioRequest>,
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

/// 404 and 409 rather than one failure, because they mean different things to
/// whoever is calling: a wrong id, against a source that exists but has its
/// audio pre-mixed by Chromium and so has nothing to balance.
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

/// Move a seekable source to a position and answer with where it landed.
async fn seek_source(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::api::SeekRequest>,
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
        // A pipeline that took the request and refused it is an ordinary
        // failure, so it answers 400 like every other one.
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
    app.mixer.request(|ack| Command::ReconnectOutput(id, Some(ack))).await?;
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
    app.mixer.request(|ack| Command::AddOutput(Box::new(cfg), Some(ack))).await?;
    Ok(StatusCode::OK)
}

async fn remove_output(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    app.mixer.request(|ack| Command::RemoveOutput(id, Some(ack))).await?;
    Ok(StatusCode::OK)
}

/// The compact document an agent reads instead of `/api/status`.
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
pub fn client_key(req: &Request) -> String {
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

/// `sheet.jpg`, `program.jpg` or `{source_id}.jpg`, with the plain text
/// errors and the status codes the old clients branch on.
async fn snapshot_image(
    State(snapshots): State<Arc<Tracker>>,
    Path(name): Path<String>,
    Query(q): Query<SnapshotQuery>,
    req: Request,
) -> Response {
    let plain =
        |code: StatusCode, msg: String| (code, [(header::CACHE_CONTROL, "no-store")], msg).into_response();
    let ask = Ask { width: q.width, force: q.force, allow_large: q.allow_large };
    match snapshot_bytes(&snapshots, &client_key(&req), &name, &ask).await {
        Ok(jpeg) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "no-store")],
            jpeg,
        )
            .into_response(),
        Err(e) if e.code == ErrorCode::NotFound.number() => {
            plain(StatusCode::NOT_FOUND, e.message)
        }
        Err(e) if e.code == ErrorCode::NotInState.number() => {
            plain(StatusCode::SERVICE_UNAVAILABLE, e.message)
        }
        Err(e) if e.code == ErrorCode::InvalidParams.number() => {
            plain(StatusCode::BAD_REQUEST, e.message)
        }
        Err(e) if e.code == ErrorCode::Safety.number() => {
            plain(StatusCode::TOO_MANY_REQUESTS, e.message)
        }
        Err(e) => plain(StatusCode::INTERNAL_SERVER_ERROR, e.message),
    }
}

// --- the background task and the per connection clock ---------------------

/// The programme running time, carried forward between status snapshots.
///
/// A status is published on a change rather than on a tick, so the number in
/// the last one goes stale. Programme running time advances with the pipeline
/// clock, which advances with wall time, so adding the elapsed time since the
/// snapshot keeps the frame header monotonic and close enough for a client
/// lining a frame up against a layout.
pub struct RunningTime {
    base_ms: u64,
    at: std::time::Instant,
    layout: u32,
}

impl Default for RunningTime {
    fn default() -> Self {
        Self { base_ms: 0, at: std::time::Instant::now(), layout: 0 }
    }
}

impl RunningTime {
    pub fn observe(&mut self, event: &Event) {
        if let Event::Status(status) = event {
            self.base_ms = status.running_time_ms;
            self.at = std::time::Instant::now();
            self.layout = crate::api::rpc::layout_id(&status.multiview.cells);
        }
    }

    pub fn now_ms(&self) -> u64 {
        self.base_ms.saturating_add(self.at.elapsed().as_millis() as u64)
    }

    /// The layout id of the grid the last status described. A frame header
    /// carries it so a client can tell which grid a late frame belongs to.
    pub fn layout(&self) -> u32 {
        self.layout
    }
}

/// Write every take down, whoever made it.
fn spawn_history(app: AppState) {
    tokio::spawn(async move {
        let mut events = app.mixer.subscribe();
        loop {
            match events.recv().await {
                Ok(envelope) => {
                    if let Event::Took { source, at_running_time_ms } = envelope.event {
                        app.history.record_event(source, at_running_time_ms, envelope.seq);
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
                Err(broadcast::error::RecvError::Lagged(_)) => {}
            }
        }
    });
}

pub async fn serve(bind: &str, state: AppState) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(%bind, "control server listening");
    let snapshots =
        Tracker::new(state.snapshot.clone(), state.multiview.clone(), state.mixer.clone());
    spawn_history(state.clone());
    let observe = crate::observe::router(observe_state(&state));
    // Connect info so the snapshot rate limit can tell one client from
    // another. Nothing else uses it, and a request without it still works.
    let app = router(state, snapshots)
        .merge(observe)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();
    axum::serve(listener, app).await?;
    Ok(())
}

/// What `/metrics`, `log.set` and the pipeline introspection routes need.
/// Everything else about them lives in `src/observe/`.
fn observe_state(state: &AppState) -> crate::observe::ObserveState {
    crate::observe::ObserveState {
        mixer: Some(state.mixer.clone()),
        multiview: Some(state.multiview.clone()),
        tokens: Some(state.tokens.clone()),
        // Prometheus scrapes with no credentials. See the field's own note.
        metrics_open: true,
        config_path: crate::config::path_in_force(std::path::Path::new("godwinmix.toml")),
    }
}

#[cfg(test)]
mod tests;
