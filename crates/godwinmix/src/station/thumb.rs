//! `GET /api/v1/shows/{id}/thumbnail.jpg?width=160`: one show's picture for
//! the monitoring wall.
//!
//! A show without compositing has no process, so its picture is the direct
//! host's: the vitals decode keyframes alone, about one a second, and only
//! for a show someone asked a picture of in the last little while
//! (`direct.thumbnail`, a `tool.call` the host answers with the JPEG in
//! base64). A show that composites is asked over the public protocol, with
//! `program.thumbnail`, which answers in the same shape from a branch on its
//! raw programme tee: one frame a second, scaled to 320 wide, for ten
//! seconds after an ask, and no mosaic. Nothing here runs unless a page is
//! looking.

use super::state::Station;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(3);

fn refusal(status: StatusCode, e: RpcError) -> Response {
    (status, Json(e.body(&godwinmix_protocol::trace::new_id()))).into_response()
}

fn jpeg(bytes: Vec<u8>) -> Response {
    (StatusCode::OK, [(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "no-store")], bytes).into_response()
}

fn not_yet(id: &str, why: &str) -> Response {
    let e = RpcError::not_in_state(format!("show {id} has no picture yet: {why}. Ask again in a second.")).with("show", id).with("retry_after_ms", 1000);
    refusal(StatusCode::CONFLICT, e)
}

pub async fn thumbnail(State(st): State<Arc<Station>>, Path(id): Path<String>, Query(q): Query<HashMap<String, String>>, headers: HeaderMap, uri: Uri) -> Response {
    let presented = crate::control::presented_token(&Method::GET, &headers, &uri);
    match st.tokens.authenticate(presented.as_deref()) {
        Ok(t) if t.has(Scope::Read) => {}
        Ok(t) => return refusal(StatusCode::FORBIDDEN, RpcError::scope("show.thumbnail", Scope::Read.as_str(), &t.scope_names())),
        Err(f) => return refusal(StatusCode::UNAUTHORIZED, RpcError::new(godwinmix_protocol::ErrorCode::Scope, f.message().to_string())),
    }
    let width = q.get("width").and_then(|w| w.parse::<u32>().ok()).unwrap_or(160).clamp(32, 1280);
    let (direct, ids) = {
        let reg = st.registry.lock();
        (reg.get(&id).map(|r| !r.compositing), reg.ids())
    };
    match direct {
        None => refusal(StatusCode::NOT_FOUND, RpcError::not_found("show", &id, &ids)),
        Some(true) => from_host(&st, &id, width).await,
        Some(false) => from_show(&st, &id, width).await,
    }
}

async fn from_host(st: &Arc<Station>, id: &str, width: u32) -> Response {
    let Some(plugins) = st.direct.plugins().cloned().filter(|p| p.is_running(crate::channels::PLUGIN)) else {
        return not_yet(id, "the ingest plugin, which runs shows without compositing, is not running");
    };
    let args = json!({"name": "direct.thumbnail", "arguments": {"show": id, "width": width}});
    let asked = tokio::task::spawn_blocking(move || plugins.call_provide(crate::channels::PLUGIN, "discover", "tool.call", args));
    let answer: Value = match tokio::time::timeout(WAIT, asked).await {
        Ok(Ok(Ok(v))) => v,
        Ok(Ok(Err(e))) => return not_yet(id, &format!("the direct host said {e:#}")),
        _ => return not_yet(id, "the direct host did not answer in time"),
    };
    picture(id, &answer, "the direct host")
}

/// A `{jpeg, width, height, at_ms}` answer as the JPEG, or the state it is
/// in: `{pending}` while the first picture is on its way, `{status: 404}` for
/// a show the host does not run.
fn picture(id: &str, answer: &Value, who: &str) -> Response {
    if let Some(b64) = answer["jpeg"].as_str() {
        return match base64::engine::general_purpose::STANDARD.decode(b64) {
            Ok(bytes) => jpeg(bytes),
            Err(_) => not_yet(id, &format!("{who} sent a picture that would not decode")),
        };
    }
    if answer["status"] == 404 {
        let why = answer["why"].as_str().unwrap_or("the direct host does not run it");
        return refusal(StatusCode::NOT_FOUND, RpcError::not_in_state(format!("show {id} has no picture: {why}.")).with("show", id));
    }
    not_yet(id, "the first picture is on its way")
}

async fn from_show(st: &Arc<Station>, id: &str, width: u32) -> Response {
    // A show that is stopped or failed says so, with the call that starts it;
    // one still starting is not waited for longer than a picture is.
    let addr = match tokio::time::timeout(WAIT, st.addr_of(id)).await {
        Ok(Ok(addr)) => addr,
        Ok(Err(e)) => return refusal(StatusCode::CONFLICT, e),
        Err(_) => {
            let e = RpcError::not_in_state(format!("show {id} has no picture yet: it is still starting. Ask again in a second."))
                .with("show", id).with("state", "starting").with("retry_after_ms", 1000);
            return refusal(StatusCode::CONFLICT, e);
        }
    };
    let secret = st.procs.lock().get(id).map(|p| p.secret.clone()).unwrap_or_default();
    let asked = st.http.get(format!("http://{addr}/api/v1/program/thumbnail?width={width}")).bearer_auth(secret).timeout(WAIT).send().await;
    let answer: Value = match asked {
        Ok(r) if r.status().is_success() => match r.json().await {
            Ok(v) => v,
            Err(_) => return not_yet(id, "its picture was cut off"),
        },
        Ok(r) => return not_yet(id, &format!("its program.thumbnail answered {}", r.status())),
        Err(_) => return not_yet(id, "it did not answer in time"),
    };
    picture(id, &answer, "the show")
}
