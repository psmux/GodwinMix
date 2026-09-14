//! The HTTP surface: `/metrics`, `log.set`, `log.gst` and the pipeline
//! introspection calls, as one router the control plane merges in.
//!
//! Mounted with a single line in `control::serve`, so nothing else in the
//! control plane knows these exist. The api agent's `/rpc` dispatcher calls
//! the same functions in `introspect` and `logs` for the JSON-RPC spelling of
//! each; this module is the REST spelling, which is what `curl` and
//! `gmx dot cam1 | dot -Tsvg` use.

use crate::mixer::MixerHandle;
use crate::observe::{doctor, introspect, logs, metrics, session, trace};
use axum::extract::{Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct ObserveState {
    /// `None` in a test that wants the routes without a pipeline behind them.
    /// Only `/metrics` uses it, to sample the status gauges at scrape time.
    pub mixer: Option<MixerHandle>,
    /// The mosaic broadcast, so a scrape can say how many clients are on it
    /// without a second counter kept in step by hand.
    pub frames: Option<Arc<tokio::sync::broadcast::Sender<Arc<[u8]>>>>,
    /// The same bearer token the rest of the control plane uses. `None` leaves
    /// these routes as open as the rest of it.
    pub token: Option<Arc<str>>,
    /// Whether `/metrics` answers without the token.
    ///
    /// On by default, because a Prometheus server scrapes with no credentials
    /// and an operator who has to configure one will not graph anything. An
    /// operator whose control port is reachable from somewhere they do not
    /// trust turns it off, and then the scrape carries the token like
    /// everything else.
    pub metrics_open: bool,
    /// The config in force, for `gmx doctor` over HTTP and the support bundle.
    pub config_path: PathBuf,
}

/// Every observability route, ready to merge into the control plane's router.
pub fn router(state: ObserveState) -> Router {
    let metrics_open = state.metrics_open;
    let guarded = Router::new()
        .route("/api/v1/log/set", post(log_set))
        .route("/api/v1/log/gst", post(log_gst))
        .route("/api/v1/log/levels", get(log_levels))
        .route("/api/v1/pipeline/dot", get(pipeline_dot))
        .route("/api/v1/pipeline/latency", get(pipeline_latency))
        .route("/api/v1/pipeline/queues", get(pipeline_queues))
        .route("/api/v1/pipeline/clock", get(pipeline_clock))
        .route("/api/v1/pipeline/list", get(pipeline_list))
        .route("/api/v1/core/startup_report", get(startup_report))
        .route("/api/v1/core/doctor", get(core_doctor))
        .route("/api/v1/core/session_log", get(session_log))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), require_token));

    let metrics = Router::new().route("/metrics", get(scrape));
    let metrics = if metrics_open {
        metrics
    } else {
        metrics.route_layer(axum::middleware::from_fn_with_state(state.clone(), require_token))
    };

    Router::new()
        .merge(guarded)
        .merge(metrics)
        .layer(axum::middleware::from_fn(trace_middleware))
        .with_state(state)
}

/// Everything a request needs to be correlated: a trace id from the caller's
/// `traceparent` when there is one, in a task local so every log line the call
/// produces carries it, and in a request extension so a handler can read it.
///
/// Also the `traceparent` on the way out, which is what lets a caller that
/// already has tracing stitch our spans onto theirs.
pub async fn trace_middleware(mut req: Request, next: Next) -> Response {
    let header = req
        .headers()
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let id = trace::incoming(header.as_deref(), None);
    req.extensions_mut().insert(id);
    let mut response = trace::with_trace_id(id, next.run(req)).await;
    if let Ok(value) = id.to_traceparent().parse() {
        response.headers_mut().insert("traceparent", value);
    }
    response
}

/// Count and time every call. The api agent's `/rpc` router wraps itself with
/// this; see `observe::rpc_layer`.
pub async fn rpc_metrics(req: Request, next: Next) -> Response {
    // The method label is the route's path, not the full URI: a query string
    // holds ids, and a label with unbounded values is how a Prometheus server
    // runs out of memory.
    let method = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    let started = std::time::Instant::now();
    let response = next.run(req).await;
    let code = response.status().as_u16().to_string();
    metrics::record_rpc(&method, &code, started.elapsed().as_secs_f64() * 1000.0);
    response
}

async fn require_token(State(state): State<ObserveState>, req: Request, next: Next) -> Response {
    let Some(token) = state.token.as_deref() else {
        return next.run(req).await;
    };
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    // Constant time is not the point here (the token is not a password hash
    // and the comparison is against a value the caller already chose), but a
    // length check first keeps the comparison cheap.
    if presented.is_some_and(|p| p.len() == token.len() && p == token) {
        return next.run(req).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(json!({ "error": "this endpoint needs the control token" })),
    )
        .into_response()
}

// --- handlers ----------------------------------------------------------------

/// `GET /metrics`, Prometheus text exposition format.
async fn scrape(State(state): State<ObserveState>) -> Response {
    // The two numbers nothing else samples: how many clients are on the
    // mosaic, and what the status says right now. Taken at scrape time so that
    // a mixer nobody is scraping does no work for metrics at all, which is
    // principle two.
    if let Some(mixer) = &state.mixer {
        if let Ok(status) = mixer.status().await {
            metrics::observe_status(&status);
        }
    }
    if let Some(frames) = &state.frames {
        metrics::set_multiview_subscribers(frames.receiver_count());
    }
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        metrics::render(),
    )
        .into_response()
}

#[derive(Deserialize)]
struct LogSet {
    /// A plugin instance: a source or an output id.
    instance: Option<String>,
    /// A module path prefix, `godwinmix::mixer`.
    target: Option<String>,
    /// `off`, `error`, `warn`, `info`, `debug` or `trace`. Also the word
    /// `default` for "stop overriding this one".
    level: String,
}

async fn log_set(Json(body): Json<LogSet>) -> Response {
    let level = if body.level.eq_ignore_ascii_case("default") {
        None
    } else {
        match logs::LevelCode::parse(&body.level) {
            Some(l) => Some(l),
            None => return bad_request(format!(
                "'{}' is not a level. Use off, error, warn, info, debug, trace, or default",
                body.level
            )),
        }
    };
    match (&body.instance, &body.target) {
        (None, None) => match level {
            Some(level) => logs::set_default_level(level),
            None => return bad_request("name an instance or a target, or give a level to set the default to"),
        },
        (Some(instance), None) => logs::set_instance_level(instance, level),
        (None, Some(target)) => logs::set_target_level(target, level),
        (Some(_), Some(_)) => {
            return bad_request("set an instance or a target, not both: the two would contradict")
        }
    }
    session::session().record(
        "log.set",
        json!({ "instance": body.instance, "target": body.target, "level": body.level }),
    );
    Json(logs::levels()).into_response()
}

#[derive(Deserialize)]
struct LogGst {
    instance: Option<String>,
    /// `GST_DEBUG` spelling: `rtmp2src:6,rtpjitterbuffer:5`.
    categories: String,
    /// How long before it goes back down. Defaults to a minute, which is long
    /// enough to reproduce a fault and short enough that a forgotten firehose
    /// stops on its own.
    #[serde(default = "default_gst_secs")]
    duration_secs: u64,
}

fn default_gst_secs() -> u64 {
    60
}

async fn log_gst(Json(body): Json<LogGst>) -> Response {
    match logs::set_gst_debug(body.instance.as_deref(), &body.categories, body.duration_secs) {
        Ok(applied) => {
            session::session().record(
                "log.gst",
                json!({
                    "instance": body.instance,
                    "categories": body.categories,
                    "duration_secs": body.duration_secs,
                }),
            );
            Json(json!({ "categories": applied, "duration_secs": body.duration_secs }))
                .into_response()
        }
        Err(e) => bad_request(format!("{e:#}")),
    }
}

async fn log_levels() -> Response {
    Json(json!({
        "levels": logs::levels(),
        "gst": logs::gst_debug_in_force()
            .into_iter()
            .map(|(name, secs)| json!({ "category": name, "secs_left": secs }))
            .collect::<Vec<_>>(),
    }))
    .into_response()
}

#[derive(Deserialize)]
struct PipelineQuery {
    /// A source or output id, or `programme`, or `multiview`.
    #[serde(default = "default_pipeline")]
    name: String,
}

fn default_pipeline() -> String {
    introspect::PROGRAMME.to_string()
}

/// `GET /api/v1/pipeline/dot?name=cam1`, which is what `gmx dot cam1` calls.
///
/// Answers `text/vnd.graphviz` rather than JSON so that piping it straight
/// into `dot -Tsvg` works with no unwrapping step.
async fn pipeline_dot(Query(q): Query<PipelineQuery>) -> Response {
    match introspect::dot(&q.name) {
        Ok(text) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/vnd.graphviz; charset=utf-8")],
            text,
        )
            .into_response(),
        Err(e) => not_found(format!("{e:#}")),
    }
}

async fn pipeline_latency(Query(q): Query<PipelineQuery>) -> Response {
    match introspect::latency(&q.name) {
        Ok(report) => Json(report).into_response(),
        Err(e) => not_found(format!("{e:#}")),
    }
}

async fn pipeline_queues(Query(q): Query<PipelineQuery>) -> Response {
    match introspect::queues(&q.name) {
        Ok(queues) => Json(json!({ "pipeline": q.name, "queues": queues })).into_response(),
        Err(e) => not_found(format!("{e:#}")),
    }
}

async fn pipeline_clock() -> Response {
    match introspect::clock() {
        Ok(report) => Json(report).into_response(),
        Err(e) => not_found(format!("{e:#}")),
    }
}

async fn pipeline_list() -> Response {
    Json(json!({ "pipelines": introspect::names() })).into_response()
}

async fn startup_report() -> Response {
    Json(introspect::startup_report()).into_response()
}

async fn core_doctor(State(state): State<ObserveState>) -> Response {
    // Every check is a syscall or a registry lookup, none of it long, but
    // none of it belongs on the runtime's worker either.
    let path = state.config_path.clone();
    match tokio::task::spawn_blocking(move || doctor::run(&path)).await {
        Ok(checks) => {
            let failed = doctor::exit_code(&checks) != 0;
            Json(json!({ "checks": checks, "ok": !failed })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
            .into_response(),
    }
}

#[derive(Deserialize)]
struct SessionQuery {
    /// How far back to read. An hour by default, which is what the support
    /// bundle wants.
    #[serde(default = "default_session_secs")]
    secs: u64,
}

fn default_session_secs() -> u64 {
    3600
}

/// The recent session log as JSON lines, for `gmx support-bundle` against a
/// mixer on another machine.
async fn session_log(Query(q): Query<SessionQuery>) -> Response {
    let lines = session::session().tail_since(q.secs.min(86_400));
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/x-ndjson; charset=utf-8")],
        lines.join("\n"),
    )
        .into_response()
}

fn bad_request(message: impl Into<String>) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message.into() }))).into_response()
}

fn not_found(message: impl Into<String>) -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": message.into() }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;

    /// The routes with no mixer behind them. Everything here is about the
    /// HTTP surface: the pipeline calls are tested against real pipelines in
    /// `introspect`, and a mixer built per test would cost an encoder probe
    /// each time for nothing.
    fn test_state() -> ObserveState {
        gstreamer::init().expect("gstreamer");
        ObserveState {
            mixer: None,
            frames: None,
            token: None,
            metrics_open: true,
            config_path: crate::observe::tempdir("routes").join("godwinmix.toml"),
        }
    }

    async fn body_text(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
        String::from_utf8_lossy(&bytes).to_string()
    }

    /// The acceptance criterion: a scrape lists the programme frame interval
    /// histogram.
    #[tokio::test]
    async fn metrics_lists_the_programme_frame_interval_histogram() {
        let state = test_state();
        let app = router(state);
        let response = app
            .oneshot(HttpRequest::builder().uri("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let text = body_text(response).await;
        assert!(text.contains("gmx_programme_frame_interval_ms"), "{text}");
        assert!(text.contains("gmx_programme_frames_total"), "{text}");
    }

    #[tokio::test]
    async fn metrics_answers_without_a_token_when_it_is_open_and_not_when_it_is_not() {
        let mut state = test_state();
        state.token = Some("secret".into());
        state.metrics_open = true;
        let open = router(state.clone());
        let response = open
            .oneshot(HttpRequest::builder().uri("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        state.metrics_open = false;
        let closed = router(state);
        let response = closed
            .oneshot(HttpRequest::builder().uri("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn log_set_moves_a_level_and_answers_with_what_is_in_force() {
        let state = test_state();
        let app = router(state);
        let response = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/api/v1/log/set")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"instance":"routes-cam","level":"debug"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let text = body_text(response).await;
        assert!(text.contains("routes-cam"), "{text}");
        assert!(text.contains("debug"), "{text}");
        logs::set_instance_level("routes-cam", None);
    }

    #[tokio::test]
    async fn a_level_nobody_can_spell_is_refused_with_the_list() {
        let state = test_state();
        let response = router(state)
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/api/v1/log/set")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"instance":"cam1","level":"chatty"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let text = body_text(response).await;
        assert!(text.contains("debug"), "the error should list the levels: {text}");
    }

    #[tokio::test]
    async fn the_programme_dot_comes_back_as_graphviz() {
        let state = test_state();
        let response = router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/pipeline/dot?name=programme")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.starts_with("text/vnd.graphviz"), "{content_type}");
        let text = body_text(response).await;
        assert!(text.contains("digraph"), "{text}");
    }

    #[tokio::test]
    async fn an_unknown_pipeline_is_a_404_naming_what_is_known() {
        let state = test_state();
        let response = router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/pipeline/dot?name=nothing-here")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(body_text(response).await.contains("Known:"));
    }

    #[tokio::test]
    async fn a_traceparent_from_the_caller_comes_back_on_the_response() {
        let state = test_state();
        let response = router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/pipeline/list")
                    .header("traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let out = response.headers().get("traceparent").unwrap().to_str().unwrap();
        assert!(out.contains("4bf92f3577b34da6a3ce929d0e0e4736"), "{out}");
    }

    #[tokio::test]
    async fn the_guarded_routes_need_the_token_and_the_right_one_gets_in() {
        let mut state = test_state();
        state.token = Some("secret".into());
        let app = router(state);
        let refused = app
            .clone()
            .oneshot(
                HttpRequest::builder().uri("/api/v1/pipeline/list").body(Body::empty()).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
        let allowed = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/pipeline/list")
                    .header("authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn the_startup_report_is_json_with_a_threshold() {
        let state = test_state();
        let response = router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/core/startup_report")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&body_text(response).await).unwrap();
        assert_eq!(v["threshold_ms"], 250.0);
        assert!(v["stages"].is_array());
    }
}
