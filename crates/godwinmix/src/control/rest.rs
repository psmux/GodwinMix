//! `/api/v1`, generated from the method table.
//!
//! There is no second list of routes. Every path comes from
//! `api::method::rest_transform`, so curl and `/rpc` reach the same handler
//! with the same params and get the same error shape back. Two routes are
//! written out by hand because they carry bytes rather than JSON: the upload,
//! whose body is the file, and the snapshot, whose answer is a JPEG.

use crate::control::call::dispatch;
use crate::control::{trace_id_of, trace_of, Ctx};
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::Registry;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub fn router(ctx: Ctx, max_upload: usize) -> Router<Ctx> {
    Router::new()
        // Bytes in and bytes out. Everything else is the generated route.
        .route(
            "/api/v1/media/upload",
            post(upload).layer(DefaultBodyLimit::max(max_upload)),
        )
        .route("/api/v1/snapshot/{id}", get(snapshot))
        // `/api/v1/status` because that is what everyone types. The method is
        // `core.status`, and `/api/v1/core/status` answers too.
        .route("/api/v1/status", get(generic))
        .route("/api/v1/{*rest}", any(generic))
        .with_state(ctx)
}

/// One path pattern from the table, split once at startup.
#[derive(Debug, Clone)]
pub struct Route {
    pub http: &'static str,
    pub method: &'static str,
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq)]
enum Segment {
    Literal(String),
    /// `{id}`, which becomes a params key of that name.
    Capture(String),
}

/// Every REST route this build answers, in the order they are tried.
///
/// Longest first, so `/api/v1/sources/{id}/audio` is preferred over
/// `/api/v1/sources/{id}` and nothing depends on the order of the table.
pub fn routes<C>(registry: &Registry<C>) -> Vec<Route> {
    let mut routes: Vec<Route> = registry
        .iter()
        .filter_map(|m| {
            m.rest.as_ref().map(|r| Route {
                http: r.http,
                method: m.name,
                segments: r
                    .path
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .map(segment)
                    .collect(),
            })
        })
        .collect();
    routes.sort_by(|a, b| {
        b.segments
            .len()
            .cmp(&a.segments.len())
            .then_with(|| a.method.cmp(b.method))
    });
    routes
}

/// The deprecated `/api/...` paths, as routes that can be resolved the same
/// way the versioned ones are.
///
/// Built so that the token check in front of them can find the method behind
/// a legacy path and apply that method's scope. Two doors onto one set of
/// methods must not mean two sets of permissions.
pub fn legacy_routes() -> Vec<Route> {
    let mut routes: Vec<Route> = godwinmix_protocol::protocol::LEGACY_ROUTES
        .iter()
        .map(|(http, path, method)| Route {
            http,
            method,
            segments: path
                .split('/')
                .filter(|s| !s.is_empty())
                .map(segment)
                .collect(),
        })
        .collect();
    routes.sort_by(|a, b| {
        b.segments
            .len()
            .cmp(&a.segments.len())
            .then_with(|| a.method.cmp(b.method))
    });
    routes
}

fn segment(raw: &str) -> Segment {
    match raw.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        Some(name) => Segment::Capture(name.to_string()),
        None => Segment::Literal(raw.to_string()),
    }
}

/// Match a request path against the table, and say what the captures were.
pub fn resolve<'a>(
    routes: &'a [Route],
    http: &Method,
    path: &str,
) -> Result<(&'a Route, Map<String, Value>), RpcError> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut path_matched = Vec::new();
    for route in routes {
        let Some(captures) = captures(route, &parts) else {
            continue;
        };
        path_matched.push(route);
        if route.http == http.as_str() {
            return Ok((route, captures));
        }
    }
    if path_matched.is_empty() {
        return Err(RpcError::new(
            ErrorCode::MethodNotFound,
            format!(
                "there is no {path} on this mixer. GET /api/v1/core/api lists every route, \
                 and the legacy /api paths still work for one release."
            ),
        )
        .with("path", path));
    }
    let allowed: Vec<String> = path_matched.iter().map(|r| r.http.to_string()).collect();
    Err(RpcError::new(
        ErrorCode::MethodNotFound,
        format!(
            "{path} does not answer {http}. It answers {}.",
            allowed.join(", ")
        ),
    )
    .with("path", path)
    .with("allowed", allowed))
}

fn captures(route: &Route, parts: &[&str]) -> Option<Map<String, Value>> {
    if route.segments.len() != parts.len() {
        return None;
    }
    let mut out = Map::new();
    for (segment, part) in route.segments.iter().zip(parts) {
        match segment {
            Segment::Literal(want) if want == part => {}
            Segment::Literal(_) => return None,
            Segment::Capture(name) => {
                out.insert(name.clone(), Value::String(decode(part)));
            }
        }
    }
    Some(out)
}

/// Percent decoding, for an id with a space or a slash in it. Small enough to
/// write out; the alternative is a crate for eight lines.
fn decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The params one HTTP request carries: the body, then the query, then the
/// path captures, each overriding the last.
///
/// The path wins because it is the most specific: `POST /sources/cam1/audio`
/// with a body naming another id is a mistake, and the path is the one the
/// caller can see in their own logs.
pub fn params_from(body: Value, query: &str, captures: Map<String, Value>) -> Value {
    let mut map = match body {
        Value::Object(map) => map,
        Value::Null => Map::new(),
        other => {
            let mut map = Map::new();
            map.insert("body".into(), other);
            map
        }
    };
    for (key, value) in query_pairs(query) {
        map.insert(key, value);
    }
    for (key, value) in captures {
        map.insert(key, value);
    }
    Value::Object(map)
}

/// Query values arrive as strings. A bare number or `true` is read as one, so
/// `?width=640` reaches a `u32` field and `?dry_run=true` reaches a bool.
fn query_pairs(query: &str) -> Vec<(String, Value)> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| {
            let (key, raw) = pair.split_once('=').unwrap_or((pair, ""));
            let value = serde_json::from_str::<Value>(&decode(raw))
                .ok()
                .filter(|v| v.is_number() || v.is_boolean())
                .unwrap_or_else(|| Value::String(decode(raw)));
            (decode(key), value)
        })
        .collect()
}

/// Everything that is not bytes: read the route, dispatch, answer.
async fn generic(State(ctx): State<Ctx>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let trace_id = trace_of(&parts.headers, None);
    let Ok(route) = resolve(&ctx.routes, &parts.method, parts.uri.path()) else {
        let e = resolve(&ctx.routes, &parts.method, parts.uri.path()).unwrap_err();
        return error_response(&e, &trace_id);
    };
    let (route, captures) = route;

    let body = match read_json(body).await {
        Ok(v) => v,
        Err(e) => return error_response(&e, &trace_id),
    };
    let params = params_from(body, parts.uri.query().unwrap_or_default(), captures);
    let id = trace_id_of(
        &parts.headers,
        params.get("trace_id").and_then(Value::as_str),
    );
    let trace_id = id.to_string();

    let token = match ctx
        .app
        .tokens
        .authenticate(bearer(&parts.headers).as_deref())
    {
        Ok(t) => t,
        Err(f) => return unauthorised(f.message(), &trace_id),
    };
    // Inside the task local, so every log line this call produces carries the
    // same id the caller is holding. See ``godwinmix_core::observe::trace``.
    match godwinmix_core::observe::with_trace_id(
        id,
        dispatch(
            &ctx.registry,
            &ctx.app,
            &ctx.snapshots,
            &token,
            &trace_id,
            route.method,
            params,
        ),
    )
    .await
    {
        Ok(mut value) => {
            if let Some(map) = value.as_object_mut() {
                map.insert("trace_id".into(), Value::String(trace_id.clone()));
            }
            (
                StatusCode::OK,
                [(header::HeaderName::from_static("x-trace-id"), trace_id)],
                Json(value),
            )
                .into_response()
        }
        Err(e) => error_response(&e, &trace_id),
    }
}

async fn read_json(body: Body) -> Result<Value, RpcError> {
    let bytes = axum::body::to_bytes(body, 1 << 20)
        .await
        .map_err(|e| RpcError::invalid_params(format!("could not read the body: {e}")))?;
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| RpcError::new(ErrorCode::ParseError, format!("the body is not JSON: {e}")))
}

pub fn error_response(e: &RpcError, trace_id: &str) -> Response {
    let status = StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::BAD_REQUEST);
    (
        status,
        [(
            header::HeaderName::from_static("x-trace-id"),
            trace_id.to_string(),
        )],
        Json(e.body(trace_id)),
    )
        .into_response()
}

fn unauthorised(reason: &str, trace_id: &str) -> Response {
    let e = RpcError::new(
        ErrorCode::Scope,
        format!("{reason}. Send it as `Authorization: Bearer <token>`."),
    );
    (
        StatusCode::UNAUTHORIZED,
        [
            (header::WWW_AUTHENTICATE, "Bearer".to_string()),
            (
                header::HeaderName::from_static("x-trace-id"),
                trace_id.to_string(),
            ),
        ],
        Json(e.body(trace_id)),
    )
        .into_response()
}

pub fn bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// A token in the query, which is the only way a browser opening a WebSocket
/// or an `<img>` tag can carry one. GET only: a token in a POST URL ends up in
/// more logs than it should.
pub fn query_token(uri: &Uri) -> Option<String> {
    let Query(pairs) = Query::<Vec<(String, String)>>::try_from_uri(uri).ok()?;
    pairs
        .into_iter()
        .find(|(k, _)| k == "token")
        .map(|(_, v)| v)
        .filter(|v| !v.is_empty())
}

/// `GET /api/v1/snapshot/{name}`: the JPEG itself, because an `<img>` tag
/// cannot read base64 out of a JSON body.
async fn snapshot(
    State(ctx): State<Ctx>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let trace_id = trace_of(&headers, None);
    let presented = bearer(&headers).or_else(|| q.get("token").cloned());
    let token = match ctx.app.tokens.authenticate(presented.as_deref()) {
        Ok(t) => t,
        Err(f) => return unauthorised(f.message(), &trace_id),
    };
    let ask = godwinmix_core::snapshot::Ask {
        width: q.get("width").and_then(|w| w.parse::<u32>().ok()),
        force: q.get("force").is_some_and(|v| v != "false"),
        allow_large: q.get("allow_large").is_some_and(|v| v != "false"),
    };
    match crate::control::snapshot_bytes(&ctx.snapshots, &token.id, &id, &ask).await {
        Ok(jpeg) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/jpeg".to_string()),
                (header::CACHE_CONTROL, "no-store".to_string()),
                (header::HeaderName::from_static("x-trace-id"), trace_id),
            ],
            jpeg,
        )
            .into_response(),
        Err(e) => error_response(&e, &trace_id),
    }
}

/// `POST /api/v1/media/upload?name=`: the body is the file, streamed to disk.
async fn upload(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let trace_id = trace_of(&headers, None);
    let token = match ctx.app.tokens.authenticate(bearer(&headers).as_deref()) {
        Ok(t) => t,
        Err(f) => return unauthorised(f.message(), &trace_id),
    };
    if !token.has(godwinmix_protocol::scope::Scope::Operate) {
        return error_response(
            &RpcError::scope("media.upload", "operate", &token.scope_names()),
            &trace_id,
        );
    }
    let Some(name) = q.get("name") else {
        return error_response(
            &RpcError::invalid_params(
                "media.upload needs the file name in the query: \
                 POST /api/v1/media/upload?name=clip.mp4 with the file as the body.",
            ),
            &trace_id,
        );
    };
    match crate::control::store_upload(&ctx.app, name, body).await {
        Ok(value) => (
            StatusCode::OK,
            [(
                header::HeaderName::from_static("x-trace-id"),
                trace_id.clone(),
            )],
            Json(json!({
                "name": value["name"],
                "path": value["path"],
                "size_bytes": value["size_bytes"],
                "should_retry": false,
                "trace_id": trace_id,
            })),
        )
            .into_response(),
        Err(e) => error_response(&e, &trace_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routes_for_test() -> Vec<Route> {
        routes(&crate::control::methods::registry())
    }

    /// Every path in the table has to resolve back to the method it was
    /// generated from, including the two that are written out by hand.
    #[test]
    fn a_path_resolves_to_the_method_that_generated_it() {
        let routes = routes_for_test();
        let at =
            |http: Method, path: &str| resolve(&routes, &http, path).map(|(r, c)| (r.method, c));
        assert_eq!(at(Method::GET, "/api/v1/sources").unwrap().0, "source.list");
        assert_eq!(at(Method::POST, "/api/v1/sources").unwrap().0, "source.add");
        let (method, captures) = at(Method::DELETE, "/api/v1/sources/cam1").unwrap();
        assert_eq!(method, "source.remove");
        assert_eq!(captures["id"], "cam1");
        let (method, captures) = at(Method::POST, "/api/v1/sources/cam1/audio").unwrap();
        assert_eq!(method, "source.audio.set");
        assert_eq!(captures["id"], "cam1");
        assert_eq!(
            at(Method::POST, "/api/v1/program/take").unwrap().0,
            "program.take"
        );
        assert_eq!(at(Method::GET, "/api/v1/program").unwrap().0, "program.get");
        assert_eq!(at(Method::GET, "/api/v1/core/info").unwrap().0, "core.info");
        assert_eq!(
            at(Method::GET, "/api/v1/agent/state").unwrap().0,
            "agent.state"
        );
        assert_eq!(
            at(Method::GET, "/api/v1/core/status").unwrap().0,
            "core.status"
        );
    }

    /// The wrong verb on a real path says which verbs it does answer, rather
    /// than reading as "no such endpoint" and sending a client to look for a
    /// route that is right there.
    #[test]
    fn the_wrong_verb_says_which_verbs_the_path_answers() {
        let routes = routes_for_test();
        let e = resolve(&routes, &Method::PUT, "/api/v1/sources").unwrap_err();
        assert!(
            e.message.contains("GET") && e.message.contains("POST"),
            "{}",
            e.message
        );
        let e = resolve(&routes, &Method::GET, "/api/v1/nonsense").unwrap_err();
        assert!(e.message.contains("core/api"), "{}", e.message);
    }

    /// A longer path wins, or `/sources/cam1/audio` would be read as a source
    /// whose id is "cam1" with a stray segment.
    #[test]
    fn the_most_specific_route_wins() {
        let routes = routes_for_test();
        let lengths: Vec<usize> = routes.iter().map(|r| r.segments.len()).collect();
        assert!(
            lengths.windows(2).all(|w| w[0] >= w[1]),
            "routes are not longest first"
        );
    }

    #[test]
    fn the_path_beats_the_query_and_the_query_beats_the_body() {
        let captures: Map<String, Value> = [("id".to_string(), Value::String("cam1".into()))]
            .into_iter()
            .collect();
        let params = params_from(json!({ "id": "cam9", "gain": 0.5 }), "id=cam5", captures);
        assert_eq!(params["id"], "cam1", "the path is the most specific");
        assert_eq!(params["gain"], 0.5);

        // Numbers and booleans come out of a query as themselves, so ?width=640
        // reaches a u32 field.
        let params = params_from(Value::Null, "width=640&dry_run=true&id=cam+2", Map::new());
        assert_eq!(params["width"], 640);
        assert_eq!(params["dry_run"], true);
        assert_eq!(params["id"], "cam 2");
    }

    #[test]
    fn percent_encoded_ids_survive_the_path() {
        assert_eq!(decode("cam%201"), "cam 1");
        assert_eq!(decode("clip%2Emp4"), "clip.mp4");
        assert_eq!(decode("plain"), "plain");
        // A stray percent is left as it is rather than eating the next bytes.
        assert_eq!(decode("100%"), "100%");
    }
}
