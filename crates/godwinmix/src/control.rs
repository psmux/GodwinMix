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
pub mod hooks;
pub mod methods;
pub mod push;
pub mod rest;
pub mod streams;
mod upload;
pub mod ws;

use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::idempotency;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::{Confirmations, Tokens};
use godwinmix_protocol::types::{CanvasInfo, Limits, MultiviewStatus};
use godwinmix_protocol::{AddSourceRequest, GoLiveRequest, GoLiveResult, MultiviewLayout};
use godwinmix_core::config::{Config, OutputConfig, SnapshotConfig, SourceConfig};
use godwinmix_core::media::{MediaLibrary, MediaListing};
use godwinmix_core::mixer::{AudioOutcome, Command, MixerHandle, SeekOutcome, Wedged};
use godwinmix_core::multiview::MultiviewHandle;
use godwinmix_core::snapshot::{self, Ask, Pick, Tracker};
use godwinmix_core::state::{Event, MixerStatus, SourceState};
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
    /// Preview and monitoring streams: `/mjpeg`, `/pcm`, `/opus`, `/whep` and
    /// the local raw socket. Opening one through this is what builds its
    /// branch, and there is no other way to reach the bytes. See
    /// `preview/hub.rs`.
    pub preview: godwinmix_core::preview::PreviewHandle,
    /// Whether the programme encoder is running and what is holding it up.
    /// Read by `/metrics`; a WHEP session will take a lease from it.
    pub encoder: godwinmix_core::encoder::EncoderHandle,
    /// The still and motion limits, `[snapshot]` in the config.
    pub snapshot: SnapshotConfig,
    /// Ad clips available on this machine.
    pub library: Arc<MediaLibrary>,
    /// Runs file transcodes and remembers their progress.
    pub converter: Arc<godwinmix_core::convert::Converter>,
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
    /// The scene collection and everything true about it. See
    /// `godwinmix_core::scene::server`.
    pub scenes: Arc<godwinmix_core::scene::server::SceneServer>,
    /// The last audio peak per source, for `agent.state` detailed.
    pub peaks: Arc<history::Peaks>,
    /// The rules that stand in front of every take: the minimum hold, the rate
    /// limit, the flash guard and the operator watchdog. See
    /// `godwinmix_core::safety`.
    pub safety: Arc<godwinmix_core::safety::Guard>,
    /// Work that outlives the call that started it. `task.get`, `task.cancel`,
    /// and the handle `media.convert` answers with.
    pub tasks: Arc<godwinmix_core::tasks::Tasks>,
    pub features: Arc<Vec<String>>,
    pub limits: Limits,
    pub canvas: CanvasInfo,
    /// True when the core was started with `--rehearsal`.
    pub rehearsal: bool,
    /// `[plugins.<name>]` from the config, held so `plugin.settings.get` can
    /// answer and `plugin.settings.set` has something to change. The core
    /// never reads inside these tables; they belong to the plugin named.
    pub plugin_settings: Arc<std::collections::BTreeMap<String, godwinmix_core::config::Params>>,
    /// `[plugins] allow_unsigned`. Whether `plugin.add` will install something
    /// nothing signed. True unless the operator turned it off.
    pub allow_unsigned: bool,
    /// `[marketplaces] only`. Empty means every marketplace that was added.
    pub marketplaces_only: Vec<String>,
    /// Where the config was read from, so a settings change can be written
    /// back to the file a restart will read.
    pub config_path: Arc<std::path::PathBuf>,
    /// Somebody else's code, told that a thing happened, and in one case asked
    /// first. Empty on a core nobody has configured a hook on, and then every
    /// call site costs one atomic read. See `control/hooks/`.
    pub hooks: Arc<hooks::Hooks>,
    /// Every plugin instance that is not a source: services, devices and
    /// transitions, kept running as singletons. See
    /// `godwinmix_core::plugin::supervisor`.
    pub plugins: Arc<godwinmix_core::plugin::supervisor::Supervisor>,
}

/// The handles onto one running engine, gathered so `AppState::new` takes a
/// config, an engine and a flag rather than a list nobody can read.
pub struct Engine {
    pub mixer: MixerHandle,
    pub multiview: MultiviewHandle,
    pub preview: godwinmix_core::preview::PreviewHandle,
    pub encoder: godwinmix_core::encoder::EncoderHandle,
    pub library: Arc<MediaLibrary>,
    pub converter: Arc<godwinmix_core::convert::Converter>,
    pub quit: Arc<tokio::sync::Notify>,
    /// The scene collection and everything true about it. See
    /// `godwinmix_core::scene::server`.
    pub scenes: Arc<godwinmix_core::scene::server::SceneServer>,
    /// The plugin singletons. Built before the mixer thread starts so a
    /// service is up by the time the first client connects.
    pub plugins: Arc<godwinmix_core::plugin::supervisor::Supervisor>,
}

/// Tell the mixer what the armed scene is, so a preview compositor draws it.
///
/// Here rather than beside `scene.preview.set` because the stream handlers
/// need it too: opening `/mjpeg/preview` is what builds the preview, and it
/// has to know what to draw before the first frame.
pub fn push_preview(app: &AppState) {
    methods::scenes::edit::push_preview_for(app);
}

impl AppState {
    /// Everything the control plane holds, worked out from the config once.
    pub fn new(cfg: &Config, engine: Engine, rehearsal: bool) -> Self {
        let Engine {
            mixer,
            multiview,
            preview,
            encoder,
            library,
            converter,
            quit,
            scenes,
            plugins,
        } = engine;
        let tokens = cfg.tokens(rehearsal);
        let safety =
            godwinmix_core::safety::Guard::new(cfg.safety.clone(), cfg.canvas.fps.max(1) as u32);
        // The flash guard needs a luminance measurement and the telemetry
        // probes are the only thing that takes one. Binding them here is what
        // lets the guard tell a flash from a dissolve while a client is
        // subscribed, and fall back to the stricter rule while none is.
        godwinmix_core::telemetry::telemetry().bind_guard(safety.clone());
        let hooks = {
            let bus = mixer.clone();
            hooks::Hooks::new(&cfg.hooks(), Arc::new(move |event| bus.emit(event)))
        };
        Self {
            mixer,
            multiview,
            preview,
            encoder,
            snapshot: cfg.snapshot.clone(),
            library,
            converter,
            quit,
            features: Arc::new(features(cfg, &tokens, rehearsal)),
            limits: Limits {
                max_upload_bytes: cfg.media.max_upload_bytes,
                max_gain: MAX_GAIN,
                max_call_secs: godwinmix_protocol::MAX_CALL_SECS,
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
            scenes,
            peaks: Arc::new(history::Peaks::new()),
            safety,
            tasks: godwinmix_core::tasks::Tasks::new(),
            rehearsal,
            plugin_settings: Arc::new(cfg.plugins.settings.clone()),
            allow_unsigned: cfg.plugins.allow_unsigned,
            marketplaces_only: cfg.marketplaces_only(),
            config_path: Arc::new(cfg.source_path.clone()),
            hooks,
            plugins,
        }
    }

    /// The transitions a plugin has added to the built in four, for
    /// `program.take` to accept by name and for the error that lists them.
    pub fn transition_names(&self) -> Vec<String> {
        self.plugins.transition_names()
    }

    /// Write one plugin's settings back to the config file.
    ///
    /// The config is the one place a restart reads settings from, so a change
    /// that only lived in memory would be lost by the next restart and an
    /// operator would rightly call that a bug. The file is rewritten whole
    /// from the table that was parsed, so comments elsewhere in it are lost;
    /// that is said plainly in `docs/how-to/install-a-plugin.md` rather than
    /// discovered.
    pub fn save_plugin_settings(
        &self,
        name: &str,
        settings: godwinmix_core::config::Params,
    ) -> anyhow::Result<godwinmix_core::config::Params> {
        let path = self.config_path.as_path();
        if path.as_os_str().is_empty() || !path.exists() {
            // An embedded core with no config file on disk. The change applies
            // to the running instance and there is nowhere to persist it.
            return Ok(settings);
        }
        let text = std::fs::read_to_string(path)?;
        let mut document: toml::Table = toml::from_str(&text)?;
        let plugins = document
            .entry("plugins".to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let Some(table) = plugins.as_table_mut() else {
            anyhow::bail!("[plugins] in {} is not a table", path.display());
        };
        table.insert(name.to_string(), toml::Value::Table(settings.clone()));
        std::fs::write(path, toml::to_string_pretty(&document)?)?;
        Ok(settings)
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
    // Every build has these; a client branches on the feature rather than on
    // a 404 it has to provoke first.
    features.push("mjpeg".into());
    features.push("audio-monitor".into());
    if godwinmix_core::preview::whep::available() {
        features.push("whep".into());
    }
    if godwinmix_core::preview::local::supported() {
        features.push("local-preview".into());
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
        .merge(streams::router(ctx.clone()))
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
    if let Some(refusal) = legacy_refusal_at(&ctx, &token, req.method(), req.uri().path()) {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": refusal }))).into_response();
    }
    next.run(req).await
}

/// Why this token may not use this legacy path, if it may not.
fn legacy_refusal_at(
    ctx: &Ctx,
    token: &godwinmix_protocol::scope::Token,
    http: &Method,
    path: &str,
) -> Option<String> {
    let (route, _) = rest::resolve(&ctx.legacy_routes, http, path).ok()?;
    let def = ctx.registry.get(route.method)?;
    if ctx.app.rehearsal && route.method == "output.add" {
        return Some(
            "this core was started with --rehearsal and will not add an output, so nothing \
             here reaches a real destination. Start a core without --rehearsal to go on air."
                .to_string(),
        );
    }
    legacy_refusal(token, def, path)
}

/// The part of it that depends only on the token and the method, so a test
/// can reach it without building a whole router.
fn legacy_refusal(
    token: &godwinmix_protocol::scope::Token,
    def: &godwinmix_protocol::method::MethodDef<Call>,
    path: &str,
) -> Option<String> {
    if !token.has(def.scope) {
        return Some(RpcError::scope(def.name, def.scope.as_str(), &token.scope_names()).message);
    }
    // A token whose policy is `confirm = required` must not be able to remove
    // a source, drop an output or shut the mixer down through a door that has
    // no way to carry a confirm token. The deprecated routes have no envelope,
    // so the answer is to send the caller to the versioned one rather than to
    // invent a confirm round trip these clients cannot complete.
    if def.destructive && token.confirm == godwinmix_protocol::scope::ConfirmPolicy::Required {
        return Some(format!(
            "this token needs a confirmation before a destructive call, and the deprecated \
             {path} cannot carry one. Call {} on /api/v1 instead: it answers -32020 with a \
             confirm_token, and the same call carrying `confirm` goes through.",
            def.name
        ));
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
pub fn trace_id_of(headers: &HeaderMap, explicit: Option<&str>) -> godwinmix_core::observe::TraceId {
    let traceparent =
        headers.get(godwinmix_protocol::trace::TRACEPARENT).and_then(|v| v.to_str().ok());
    godwinmix_protocol::trace::incoming(traceparent, explicit)
}

/// The whole protocol document, built once.
///
/// `core.api`, `godwinmix --api-info` and the committed `protocol.json` are
/// all this value. Built from a fresh registry, so it needs no mixer and can
/// be printed on a machine with no GStreamer and no configuration.
pub fn descriptor() -> &'static Value {
    static DOC: OnceLock<Value> = OnceLock::new();
    DOC.get_or_init(|| godwinmix_protocol::protocol::descriptor(&methods::registry(), godwinmix_core::plugin::described_kinds()))
}

/// The OpenAPI 3.1 description of the REST layer, built once.
///
/// The committed `openapi.json`, and what a client generator or Swagger UI
/// reads. Built from the same table as everything else.
pub fn openapi() -> &'static Value {
    static DOC: OnceLock<Value> = OnceLock::new();
    DOC.get_or_init(|| godwinmix_protocol::openapi::openapi(&methods::registry()))
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
        Some("web") | Some("page") | Some("website") => godwinmix_core::input::as_web_uri(&uri),
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

/// `source.restore`: a removed source back under its own id, from the config
/// the mixer kept. The address never goes through a client.
pub async fn restore_source(app: &AppState, id: &str) -> Result<String> {
    let configs = app.mixer.configs().await?;
    if configs.sources.iter().any(|c| c.id == id) {
        anyhow::bail!("source {id} already exists, so there is nothing to restore. Call source.list.");
    }
    let Some(cfg) = configs.removed.iter().rev().find(|c| c.id == id).cloned() else {
        let known: Vec<&str> = configs.removed.iter().map(|c| c.id.as_str()).collect();
        anyhow::bail!(
            "no removed source {id} is remembered. The mixer keeps the last {} it removed, until \
             it restarts, and has: {}. Add it again with source.add and its uri.",
            godwinmix_core::mixer::REMOVED_KEPT,
            if known.is_empty() { "none".to_string() } else { known.join(", ") }
        );
    };
    app.mixer.request(|ack| Command::AddSource(Box::new(cfg), Some(ack))).await?;
    Ok(id.to_string())
}

/// `source.duplicate`: another source with a live one's address and settings,
/// under the first free id.
pub async fn copy_source(
    app: &AppState,
    like: &str,
    id: Option<String>,
    name: Option<String>,
) -> Result<String> {
    let configs = app.mixer.configs().await?;
    let Some(original) = configs.sources.iter().find(|c| c.id == like) else {
        let known: Vec<&str> = configs.sources.iter().map(|c| c.id.as_str()).collect();
        anyhow::bail!("no source {like} to copy. This mixer has: {}.", known.join(", "));
    };
    let name = name
        .filter(|n| !n.trim().is_empty())
        .or_else(|| original.name.as_ref().map(|n| format!("{n} copy")));
    let base_id = id.filter(|i| !i.trim().is_empty()).unwrap_or_else(|| match &name {
        Some(n) => slug(n),
        None => format!("{like}-copy"),
    });
    for id in id_candidates(&base_id) {
        let mut cfg = original.clone();
        cfg.id = id.clone();
        cfg.name = name.clone();
        match app.mixer.request(|ack| Command::AddSource(Box::new(cfg), Some(ack))).await {
            Ok(()) => return Ok(id),
            Err(e) if e.to_string().contains("already exists") => continue,
            Err(e) => return Err(e),
        }
    }
    anyhow::bail!("could not find a free id for {base_id}")
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
    let uri = godwinmix_core::input::as_web_uri(url);
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
/// in it. Written under a dotted `.part` name and published on success so a half
/// uploaded file never appears in the listing and never gets taken to air.
pub async fn store_upload(app: &AppState, name: &str, body: Body) -> Result<Value, RpcError> {
    if !app.library.cfg().allow_upload {
        return Err(RpcError::not_in_state(
            "uploads are disabled on this server. Set `allow_upload = true` under [media] \
             and restart, or put the file in the media directory yourself.",
        ));
    }
    let name = godwinmix_core::media::safe_upload_name(name)
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
    let dir = app.library.dir().to_path_buf();
    let final_path = dir.join(&name);
    let written = upload::store(&dir, &name, body).await?;

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
    // The limit is per client and per picture. A gallery asks for one still
    // per tile in the same instant, and with one bucket per client every tile
    // but the first was refused and drew as a broken image.
    let width = match snapshots.resolve(&format!("{client} {with_suffix}"), ask) {
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
        id: godwinmix_protocol::rpc::layout_id(&multiview.cells),
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
        // A mixer that has not answered is a state, not a bad request. 503
        // with the name of the command holding the loop, so the UI banner can
        // say what is wrong: `ui/boot.js` gates the whole page on this route
        // and a request that never came back drew nothing at all.
        if let Some(wedged) = self.0.downcast_ref::<Wedged>() {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": wedged.to_string(), "data": wedged.data() })),
            )
                .into_response();
        }
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
) -> Result<Json<godwinmix_core::convert::ConversionState>, ApiError> {
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
    let target = godwinmix_core::input::to_uri(&path.display().to_string());
    let configs = app.mixer.configs().await?;
    if let Some(s) = configs.sources.iter().find(|s| godwinmix_core::input::to_uri(&s.uri) == target) {
        return Err(
            anyhow::anyhow!("{name} is the source \"{}\". Remove the source first.", s.id).into()
        );
    }
    let mut removed = Vec::new();
    for p in [path.clone(), godwinmix_core::convert::converted_sibling(&path)] {
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
    Json(req): Json<godwinmix_protocol::AudioRequest>,
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
    Json(req): Json<godwinmix_protocol::SeekRequest>,
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
) -> Result<Json<Vec<godwinmix_core::state::OutputStatus>>, ApiError> {
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
            self.layout = godwinmix_protocol::rpc::layout_id(&status.multiview.cells);
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

/// The background tasks the control plane needs whatever door a call arrives
/// by: the take history, the audio peaks, and the operator watchdog.
///
/// Public because an embedder and the integration tests build an `AppState`
/// without `serve`, and a core whose history is empty answers `program.revert`
/// with "nothing to go back to" however many takes it has had.
pub fn spawn_background(app: AppState) {
    spawn_history(app.clone());
    // One subscriber turns source.state, output.state, alert.raised and
    // plugin.state into hooks. It returns at once on a core with none.
    hooks::spawn_watch(app.hooks.clone(), app.mixer.subscribe());
    spawn_operator_watchdog(app);
}

/// Write every take down, whoever made it.
fn spawn_history(app: AppState) {
    tokio::spawn(async move {
        let mut events = app.mixer.subscribe();
        loop {
            match events.recv().await {
                Ok(envelope) => match envelope.event {
                    Event::Took { source, at_running_time_ms, .. } => {
                        app.history.record_event(source, at_running_time_ms, envelope.seq);
                    }
                    // Ten a second and nothing kept them, so an agent could
                    // not find out whether a source was making a sound.
                    Event::SourceAudioLevel { source, peak_db } => {
                        app.peaks.note(&source, &peak_db);
                    }
                    Event::Status(status) => {
                        let ids: Vec<String> =
                            status.sources.iter().map(|s| s.id.clone()).collect();
                        app.peaks.retain(&ids);
                    }
                    _ => {}
                },
                Err(broadcast::error::RecvError::Closed) => return,
                // A lagged subscriber has lost events, and a lost `took` is a
                // take missing from `program.history` and from what
                // `program.revert` can go back to. Nothing here can get them
                // again; saying how many went is what makes that visible
                // instead of silent.
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(
                        missed,
                        "the take history fell behind the event bus and lost events; \
                         program.revert may not see a take that was made"
                    );
                }
            }
        }
    });
}

/// Watch whoever made the last take, and act when they go quiet.
///
/// 03 section 6: `on_operator_silence` watches the token that made the last
/// take; if it makes no RPC call for `after_secs` the core raises a `critical`
/// alert and takes the configured action. The default is `alert`, because a
/// programme that keeps running is the safe state.
fn spawn_operator_watchdog(app: AppState) {
    use godwinmix_core::safety::SilenceAction;
    let after = app.safety.config().on_operator_silence.after_secs;
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let Some(who) = app.safety.silent_operator() else { continue };
            let action = app.safety.config().on_operator_silence.action.clone();
            // Arm it before acting, so this fires once rather than every
            // second until somebody comes back.
            app.safety.arm_silence(matches!(action, SilenceAction::Hold));
            let what = match &action {
                SilenceAction::Alert => "the programme is unchanged".to_string(),
                SilenceAction::Hold => "the programme is held until somebody calls".to_string(),
                SilenceAction::Slate => "cutting to the slate".to_string(),
                SilenceAction::Fallback(id) => format!("cutting to {id}"),
            };
            let message = format!(
                "'{who}' made the last take and has made no call for {after} seconds: {what}. \
                 Any call from any token clears this."
            );
            warn!(%who, after, action = %action.as_str(), "operator silence");
            app.mixer.emit(Event::Alert {
                severity: godwinmix_protocol::types::Severity::Critical,
                message,
            });
            let target = match &action {
                SilenceAction::Slate => Some(None),
                SilenceAction::Fallback(id) => Some(Some(id.clone())),
                _ => None,
            };
            if let Some(source) = target {
                app.history.expect("core");
                let _ = app
                    .mixer
                    .request(|ack| Command::Take {
                        source,
                        at_running_time_ms: None,
                        ack: Some(ack),
                    })
                    .await;
            }
        }
    });
}

pub async fn serve(bind: &str, state: AppState) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(%bind, "control server listening");
    serve_on(listener, state).await
}

/// The same, on a listener somebody else opened.
///
/// What a test uses to get a port the operating system picked, so two of them
/// can run at once and neither has to guess a number that is free.
pub async fn serve_on(listener: tokio::net::TcpListener, state: AppState) -> Result<()> {
    let snapshots =
        Tracker::new(state.snapshot.clone(), state.multiview.clone(), state.mixer.clone());
    spawn_background(state.clone());
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
/// Everything else about them lives in `crate::observe` and in
/// `godwinmix_core::observe`.
fn observe_state(state: &AppState) -> crate::observe::ObserveState {
    crate::observe::ObserveState {
        mixer: Some(state.mixer.clone()),
        multiview: Some(state.multiview.clone()),
        preview: Some(state.preview.clone()),
        encoder: Some(state.encoder.clone()),
        tokens: Some(state.tokens.clone()),
        // Prometheus scrapes with no credentials. See the field's own note.
        metrics_open: true,
        config_path: godwinmix_core::config::path_in_force(std::path::Path::new("godwinmix.toml")),
    }
}

#[cfg(test)]
mod tests;
