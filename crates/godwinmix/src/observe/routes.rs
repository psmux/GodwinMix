//! The HTTP surface: `/metrics`, `log.set`, `log.gst` and the pipeline
//! introspection calls, as one router the control plane merges in.
//!
//! Mounted with a single line in `control::serve`, so nothing else in the
//! control plane knows these exist. The api agent's `/rpc` dispatcher calls
//! the same functions in `introspect` and `logs` for the JSON-RPC spelling of
//! each; this module is the REST spelling, which is what `curl` and
//! `gmx dot cam1 | dot -Tsvg` use.

use godwinmix_core::mixer::MixerHandle;
use godwinmix_core::observe::{doctor, introspect, logs, metrics, session, trace};
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
    /// The multiview handle, for the subscriber count and the mosaic rate.
    pub multiview: Option<godwinmix_core::multiview::MultiviewHandle>,
    /// The preview and monitoring streams, for `gmx_stream_clients{kind}`.
    pub preview: Option<godwinmix_core::preview::PreviewHandle>,
    /// The programme encoder's lifecycle, for `gmx_encoder_running` and
    /// `gmx_encoder_consumers{kind}`.
    pub encoder: Option<godwinmix_core::encoder::EncoderHandle>,
    /// The same bearer token the rest of the control plane uses. `None` leaves
    /// these routes as open as the rest of it.
    pub tokens: Option<Arc<godwinmix_protocol::scope::Tokens>>,
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

/// Every route this module answers on, in the form the method table spells
/// them. `observe::methods` declares the same list and a test compares the
/// two, so a route cannot exist without a method or answer at a different
/// address from the one `protocol.json` publishes.
pub fn served_paths() -> Vec<&'static str> {
    vec![
        "POST /api/v1/log/set",
        "POST /api/v1/log/gst",
        "GET /api/v1/log/levels",
        "GET /api/v1/pipeline/dot",
        "GET /api/v1/pipeline/latency",
        "GET /api/v1/pipeline/queues",
        "GET /api/v1/pipeline/clock",
        "GET /api/v1/pipeline/list",
        "GET /api/v1/core/startup_report",
        "GET /api/v1/core/doctor",
        "GET /api/v1/core/session_log",
    ]
}

/// Every observability route, ready to merge into the control plane's router.
///
/// These are the REST forms of the methods in `observe::methods`, and they
/// are served here rather than by the generated router because two of the
/// answers are not JSON: a dot graph is graphviz text a caller pipes into
/// `dot`, and the session log is ndjson. A specific path beats the generated
/// `/api/v1/{*rest}`, so each of these is answered once.
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
    let Some(tokens) = state.tokens.as_ref() else {
        return next.run(req).await;
    };
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    // One authenticator for the whole core: the same table, the same constant
    // time comparison and the same refusal text the rest of the API uses.
    match tokens.authenticate(presented) {
        Ok(_) => next.run(req).await,
        Err(reason) => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            Json(json!({ "error": reason.message() })),
        )
            .into_response(),
    }
}

// --- handlers ----------------------------------------------------------------

/// `GET /metrics`, Prometheus text exposition format.
async fn scrape(State(state): State<ObserveState>) -> Response {
    // The two numbers nothing else samples: how many clients are on the
    // mosaic, and what the status says right now. Taken at scrape time so that
    // a mixer nobody is scraping does no work for metrics at all, which is
    // principle two.
    if let Some(mixer) = &state.mixer {
        match mixer.status().await {
            Ok(status) => metrics::observe_status(&status),
            Err(e) => {
                // A scrape that hangs is worse than one that fails: Prometheus
                // holds the connection open and an operator watching the
                // dashboard sees nothing at all. 503 with the name of the
                // command holding the mixer loop says which one it is.
                if let Some(wedged) = e.downcast_ref::<godwinmix_core::mixer::Wedged>() {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                        format!("{wedged}\n"),
                    )
                        .into_response();
                }
            }
        }
    }
    if let Some(mv) = &state.multiview {
        let stats = mv.stats();
        metrics::set_multiview_subscribers(stats.subscribers as usize);
        metrics::set_multiview_fps(stats.fps);
    }
    // What is on a preview or monitoring stream, and whether the programme
    // encoder is running at all. Both are gauges nothing else samples, and both
    // are how an operator checks that an idle core is idle.
    if let Some(preview) = &state.preview {
        metrics::set_stream_clients(&preview.clients().counts());
    }
    if let Some(encoder) = &state.encoder {
        metrics::set_encoder(&encoder.stats());
    }
    metrics::sample_source_queues();
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
    // On a blocking worker: the file is tens of megabytes on a long show
    // and reading it on the runtime is how one slow disk becomes everyone's
    // latency.
    let lines = session::session().tail_since_async(q.secs.min(86_400)).await;
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

    /// The routes, served on an ephemeral port, with a client pointed at them.
    ///
    /// A real listener and a real HTTP client rather than calling the handlers
    /// directly: the token guard, the content types and the `traceparent`
    /// header are all things that only exist once a request has been through
    /// axum, and this is the cheapest way to test them without adding a
    /// dependency for it.
    struct Served {
        base: String,
        client: reqwest::Client,
        task: tokio::task::JoinHandle<()>,
    }

    impl Served {
        async fn start(state: ObserveState) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let base = format!("http://{}", listener.local_addr().unwrap());
            let app = router(state);
            let task = tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            Self { base, client: reqwest::Client::new(), task }
        }

        fn get(&self, path: &str) -> reqwest::RequestBuilder {
            self.client.get(format!("{}{path}", self.base))
        }

        fn post(&self, path: &str, body: &str) -> reqwest::RequestBuilder {
            self.client
                .post(format!("{}{path}", self.base))
                .header("content-type", "application/json")
                .body(body.to_string())
        }
    }

    impl Drop for Served {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    /// The routes with no mixer behind them. Everything here is about the HTTP
    /// surface; the pipeline calls are tested against real pipelines in
    /// `introspect`, and a mixer built per test would cost an encoder probe
    /// each time for nothing.
    fn test_state() -> ObserveState {
        gstreamer::init().expect("gstreamer");
        ObserveState {
            mixer: None,
            multiview: None,
            preview: None,
            encoder: None,
            tokens: None,
            metrics_open: true,
            config_path: crate::observe::tempdir("routes").join("godwinmix.toml"),
        }
    }

    /// Nothing runs unless asked, read off `/metrics`: an idle core lists every
    /// stream kind at zero and says the encoder is not running.
    #[tokio::test]
    async fn an_idle_core_reports_no_stream_clients_and_no_encoder() {
        let mut state = test_state();
        let preview = godwinmix_core::preview::PreviewHandle::detached();
        state.preview = Some(preview.clone());
        state.encoder = Some(godwinmix_core::encoder::EncoderHandle::detached(
            godwinmix_core::encoder::EncoderPolicy::OnDemand,
        ));
        let server = Served::start(state).await;
        let body = server.get("/metrics").send().await.unwrap().text().await.unwrap();
        for kind in godwinmix_core::preview::STREAM_KINDS {
            assert!(
                body.contains(&format!("gmx_stream_clients{{kind=\"{kind}\"}} 0")),
                "no zero line for {kind} in:\n{body}"
            );
        }
        assert!(body.contains("gmx_encoder_running 0"), "{body}");
        assert!(body.contains("gmx_encoder_consumers{kind=\"output\"} 0"), "{body}");
        assert!(body.contains("gmx_encoder_consumers{kind=\"whep\"} 0"), "{body}");

        // And a client shows up. In the same test because the metric registry
        // is process wide: two tests moving one gauge would flap.
        let client = preview.count("mjpeg");
        let body = server.get("/metrics").send().await.unwrap().text().await.unwrap();
        assert!(body.contains("gmx_stream_clients{kind=\"mjpeg\"} 1"), "{body}");
        assert!(body.contains("gmx_stream_clients{kind=\"pcm\"} 0"), "{body}");

        // And goes again when it leaves, rather than keeping its last value.
        drop(client);
        let body = server.get("/metrics").send().await.unwrap().text().await.unwrap();
        assert!(body.contains("gmx_stream_clients{kind=\"mjpeg\"} 0"), "{body}");
    }

    /// The acceptance criterion: a scrape lists the programme frame interval
    /// histogram.
    #[tokio::test]
    async fn metrics_lists_the_programme_frame_interval_histogram() {
        let server = Served::start(test_state()).await;
        let response = server.get("/metrics").send().await.unwrap();
        assert_eq!(response.status(), 200);
        let content_type =
            response.headers().get("content-type").unwrap().to_str().unwrap().to_string();
        assert!(content_type.starts_with("text/plain"), "{content_type}");
        let text = response.text().await.unwrap();
        assert!(text.contains("gmx_programme_frame_interval_ms"), "{text}");
        assert!(text.contains("gmx_programme_frames_total"), "{text}");
        assert!(text.contains("# TYPE gmx_programme_frame_interval_ms histogram"), "{text}");
    }

    #[tokio::test]
    async fn metrics_is_open_by_default_and_closed_when_the_operator_says_so() {
        let mut state = test_state();
        state.tokens = Some(std::sync::Arc::new(godwinmix_protocol::scope::Tokens::new(
            vec![godwinmix_protocol::scope::Token::legacy("secret")],
            false,
        )));
        state.metrics_open = true;
        let open = Served::start(state.clone()).await;
        assert_eq!(open.get("/metrics").send().await.unwrap().status(), 200);

        state.metrics_open = false;
        let closed = Served::start(state).await;
        assert_eq!(closed.get("/metrics").send().await.unwrap().status(), 401);
        let with_token =
            closed.get("/metrics").bearer_auth("secret").send().await.unwrap();
        assert_eq!(with_token.status(), 200);
    }

    #[tokio::test]
    async fn log_set_moves_a_level_and_answers_with_what_is_in_force() {
        let server = Served::start(test_state()).await;
        let response = server
            .post("/api/v1/log/set", r#"{"instance":"routes-cam","level":"debug"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let text = response.text().await.unwrap();
        assert!(text.contains("routes-cam"), "{text}");
        assert!(text.contains("debug"), "{text}");

        // And "default" puts it back, which is how an operator turns the
        // firehose off without knowing what it was before.
        let back = server
            .post("/api/v1/log/set", r#"{"instance":"routes-cam","level":"default"}"#)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(!back.contains("routes-cam"), "{back}");
    }

    #[tokio::test]
    async fn a_level_nobody_can_spell_is_refused_with_the_list() {
        let server = Served::start(test_state()).await;
        let response =
            server.post("/api/v1/log/set", r#"{"instance":"cam1","level":"chatty"}"#).send().await.unwrap();
        assert_eq!(response.status(), 400);
        let text = response.text().await.unwrap();
        assert!(text.contains("debug"), "the error should list the levels: {text}");
    }

    #[tokio::test]
    async fn setting_an_instance_and_a_target_at_once_is_refused_rather_than_guessed() {
        let server = Served::start(test_state()).await;
        let response = server
            .post("/api/v1/log/set", r#"{"instance":"cam1","target":"godwinmix","level":"debug"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn log_gst_raises_a_category_and_says_for_how_long() {
        let server = Served::start(test_state()).await;
        let response = server
            .post("/api/v1/log/gst", r#"{"categories":"rtmp2src:4","duration_secs":1}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let v: serde_json::Value = response.json().await.unwrap();
        assert_eq!(v["categories"][0], "rtmp2src");
        assert_eq!(v["duration_secs"], 1);

        let levels: serde_json::Value =
            server.get("/api/v1/log/levels").send().await.unwrap().json().await.unwrap();
        assert!(levels["gst"].as_array().unwrap().iter().any(|c| c["category"] == "rtmp2src"));
    }

    #[tokio::test]
    async fn a_category_that_is_not_gst_debugs_spelling_is_refused() {
        let server = Served::start(test_state()).await;
        let response = server
            .post("/api/v1/log/gst", r#"{"categories":"rtmp2src"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        assert!(response.text().await.unwrap().contains("<category>:<level>"));
    }

    #[tokio::test]
    async fn the_programme_dot_comes_back_as_graphviz_ready_to_pipe() {
        let server = Served::start(test_state()).await;
        let pipeline = gstreamer::Pipeline::with_name("routes-programme");
        godwinmix_core::observe::register_pipeline(godwinmix_core::observe::PROGRAMME, &pipeline);

        let response = server.get("/api/v1/pipeline/dot?name=programme").send().await.unwrap();
        assert_eq!(response.status(), 200);
        let content_type =
            response.headers().get("content-type").unwrap().to_str().unwrap().to_string();
        assert!(content_type.starts_with("text/vnd.graphviz"), "{content_type}");
        let text = response.text().await.unwrap();
        assert!(text.contains("digraph"), "{text}");
        godwinmix_core::observe::unregister_pipeline(godwinmix_core::observe::PROGRAMME);
    }

    #[tokio::test]
    async fn an_unknown_pipeline_is_a_404_naming_what_is_known() {
        let server = Served::start(test_state()).await;
        let response = server.get("/api/v1/pipeline/dot?name=nothing-here").send().await.unwrap();
        assert_eq!(response.status(), 404);
        assert!(response.text().await.unwrap().contains("Known:"));
    }

    #[tokio::test]
    async fn a_traceparent_from_the_caller_comes_back_on_the_response() {
        let server = Served::start(test_state()).await;
        let response = server
            .get("/api/v1/pipeline/list")
            .header("traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
            .send()
            .await
            .unwrap();
        let out = response.headers().get("traceparent").unwrap().to_str().unwrap().to_string();
        assert!(out.contains("4bf92f3577b34da6a3ce929d0e0e4736"), "{out}");
    }

    #[tokio::test]
    async fn a_request_with_no_traceparent_still_gets_one() {
        let server = Served::start(test_state()).await;
        let response = server.get("/api/v1/pipeline/list").send().await.unwrap();
        let out = response.headers().get("traceparent").unwrap().to_str().unwrap().to_string();
        assert!(godwinmix_core::observe::TraceId::from_traceparent(&out).is_some(), "{out}");
    }

    #[tokio::test]
    async fn the_guarded_routes_need_the_token_and_the_right_one_gets_in() {
        let mut state = test_state();
        state.tokens = Some(std::sync::Arc::new(godwinmix_protocol::scope::Tokens::new(
            vec![godwinmix_protocol::scope::Token::legacy("secret")],
            false,
        )));
        let server = Served::start(state).await;
        assert_eq!(server.get("/api/v1/pipeline/list").send().await.unwrap().status(), 401);
        assert_eq!(
            server.get("/api/v1/pipeline/list").bearer_auth("wrong").send().await.unwrap().status(),
            401
        );
        assert_eq!(
            server.get("/api/v1/pipeline/list").bearer_auth("secret").send().await.unwrap().status(),
            200
        );
    }

    #[tokio::test]
    async fn the_startup_report_is_json_with_a_threshold() {
        let server = Served::start(test_state()).await;
        let v: serde_json::Value =
            server.get("/api/v1/core/startup_report").send().await.unwrap().json().await.unwrap();
        assert_eq!(v["threshold_ms"], 250.0);
        assert!(v["stages"].is_array());
    }

    #[tokio::test]
    async fn the_doctor_answers_over_http_with_a_verdict_per_check() {
        let server = Served::start(test_state()).await;
        let v: serde_json::Value =
            server.get("/api/v1/core/doctor").send().await.unwrap().json().await.unwrap();
        let checks = v["checks"].as_array().expect("checks");
        assert!(checks.len() >= 6, "{v}");
        for check in checks {
            assert!(check["verdict"].is_string(), "{check}");
            assert!(check["detail"].is_string(), "{check}");
        }
    }

    /// `rpc_layer!` is what the api agent wraps `/rpc` with, so it is tested
    /// the way they will use it.
    #[tokio::test]
    async fn the_rpc_layer_counts_and_times_a_call() {
        let app: Router = Router::new()
            .route("/rpc/{method}", axum::routing::post(|| async { "{}" }))
            .layer(crate::rpc_layer!());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let client = reqwest::Client::new();
        client.post(format!("{base}/rpc/program.take")).send().await.unwrap();
        task.abort();

        let text = metrics::render();
        assert!(
            text.contains(r#"gmx_rpc_calls_total{code="200",method="/rpc/{method}"}"#),
            "the method label should be the route, not the path: {text}"
        );
        assert!(text.contains("gmx_rpc_duration_ms_count"), "{text}");
    }
}
