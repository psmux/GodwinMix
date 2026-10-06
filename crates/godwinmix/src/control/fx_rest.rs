//! The two fx routes that carry bytes: an item's preview strip out, and a
//! pack in from a browser's drop.
//!
//! `GET /api/v1/fx/{name}/preview.jpg` is the JPEG `fx.preview` names, made
//! the first time it is asked for. `POST /api/v1/fx/upload?name=pack.zip`
//! takes the file as the body, keeps it under the library's `.uploads`
//! folder and imports it, answering what `fx.import` answers. Both read the
//! token the way the snapshot and the media upload do.

use crate::control::rest::{bearer, error_response, unauthorised};
use crate::control::{trace_of, Ctx};
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use godwinmix_core::fx::{library, sprite};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::fx::FxImportRequest;
use godwinmix_protocol::scope::Scope;
use std::collections::HashMap;

pub async fn preview(State(ctx): State<Ctx>, Path(name): Path<String>, Query(q): Query<HashMap<String, String>>, headers: HeaderMap) -> Response {
    let trace_id = trace_of(&headers, None);
    let presented = bearer(&headers).or_else(|| q.get("token").cloned());
    if let Err(f) = ctx.app.tokens.authenticate(presented.as_deref()) {
        return unauthorised(f.message(), &trace_id);
    }
    let dir = godwinmix_core::gallery::dir();
    let name = name.trim_end_matches(".jpg").to_string();
    let made = tokio::task::spawn_blocking(move || {
        let (m, folder) = library::find(&dir, &name)?;
        let path = sprite::ensure(&m, &folder)?;
        Ok::<_, anyhow::Error>(std::fs::read(path)?)
    })
    .await;
    match made {
        Ok(Ok(jpeg)) => (StatusCode::OK, [(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "max-age=60")], jpeg).into_response(),
        Ok(Err(e)) => error_response(&RpcError::invalid_params(format!("{e:#}")).with("list", "fx.list"), &trace_id),
        Err(e) => error_response(&RpcError::internal(format!("{e}")), &trace_id),
    }
}

pub async fn upload(State(ctx): State<Ctx>, Query(q): Query<HashMap<String, String>>, headers: HeaderMap, body: Body) -> Response {
    let trace_id = trace_of(&headers, None);
    let token = match ctx.app.tokens.authenticate(bearer(&headers).as_deref()) {
        Ok(t) => t,
        Err(f) => return unauthorised(f.message(), &trace_id),
    };
    if !token.has(Scope::Operate) {
        return error_response(&RpcError::scope("fx.import", "operate", &token.scope_names()), &trace_id);
    }
    let Some(name) = q.get("name").map(|n| n.rsplit(['/', '\\']).next().unwrap_or_default().to_string()).filter(|n| !n.is_empty() && !n.starts_with('.')) else {
        return error_response(&RpcError::invalid_params("fx upload needs the file name in the query: POST /api/v1/fx/upload?name=pack.zip with the file as the body."), &trace_id);
    };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let dir = godwinmix_core::gallery::dir().join(".uploads").join(stamp.to_string());
    if let Err(e) = crate::control::upload::store(&dir, &name, body).await {
        return error_response(&e, &trace_id);
    }
    let path = dir.join(&name).display().to_string();
    let req = FxImportRequest { path, replace: q.get("replace").is_some_and(|v| v == "true"), ..Default::default() };
    let answer = crate::control::methods::fx::import_now(&ctx.app, req).await;
    // A task still reads the file; anything else is done with it.
    if answer.as_ref().map(|v| v.get("task_id").is_none()).unwrap_or(true) {
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
    match answer {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(e) => error_response(&e, &trace_id),
    }
}
