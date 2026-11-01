//! The LUT route that carries bytes: a LUT file in from a browser's pick or
//! drop.
//!
//! `POST /api/v1/luts/upload?name=Kodak%202383.cube` takes the file as the
//! body, keeps it under the library's `.uploads` folder for as long as the
//! import takes, and answers what `lut.import` answers. A `title` in the
//! query names it in the UI. It reads the token the way the fx upload does.

use crate::control::rest::{bearer, error_response, unauthorised};
use crate::control::{trace_of, Ctx};
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use godwinmix_core::lut::library;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::lut::LutImported;
use godwinmix_protocol::scope::Scope;
use std::collections::HashMap;

pub async fn upload(State(ctx): State<Ctx>, Query(q): Query<HashMap<String, String>>, headers: HeaderMap, body: Body) -> Response {
    let trace_id = trace_of(&headers, None);
    let token = match ctx.app.tokens.authenticate(bearer(&headers).as_deref()) {
        Ok(t) => t,
        Err(f) => return unauthorised(f.message(), &trace_id),
    };
    if !token.has(Scope::Operate) {
        return error_response(&RpcError::scope("lut.import", "operate", &token.scope_names()), &trace_id);
    }
    let Some(name) = q.get("name").map(|n| n.rsplit(['/', '\\']).next().unwrap_or_default().to_string()).filter(|n| !n.is_empty() && !n.starts_with('.')) else {
        return error_response(&RpcError::invalid_params("a LUT upload needs the file name in the query: POST /api/v1/luts/upload?name=look.cube with the file as the body."), &trace_id);
    };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let dir = library::dir().join(".uploads").join(stamp.to_string());
    if let Err(e) = crate::control::upload::store(&dir, &name, body).await {
        return error_response(&e, &trace_id);
    }
    let (path, title) = (dir.join(&name), q.get("title").cloned());
    let made = tokio::task::spawn_blocking(move || library::import(&library::dir(), &path, None, title.as_deref())).await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
    match made {
        Ok(Ok(lut)) => (StatusCode::OK, Json(LutImported { lut })).into_response(),
        Ok(Err(e)) => error_response(
            &RpcError::invalid_params(format!("{e:#}")).with("formats", serde_json::json!(godwinmix_core::lut::FORMATS)),
            &trace_id,
        ),
        Err(e) => error_response(&RpcError::internal(format!("{e}")), &trace_id),
    }
}
