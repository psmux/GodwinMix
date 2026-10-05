//! The gallery's byte routes: a file of an item (an HTML graphic's page for
//! the browser source, a clip for a card's moving preview), a dropped or
//! picked file taken in, and an export downloaded.
//!
//! A file of an item is served without a token to a process on this
//! machine, because the browser source that draws an HTML graphic is one
//! and carries no token. Anyone else needs the token, as for any read.

use crate::control::rest::{bearer, error_response};
use crate::control::{trace_of, Ctx};
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use godwinmix_core::gallery::{self, bundle};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::scope::Scope;
use std::collections::HashMap;
use std::net::SocketAddr;

/// `GET /api/v1/gallery/{id}/files/{*path}`.
pub async fn serve_file(State(ctx): State<Ctx>, Path((id, path)): Path<(String, String)>, Query(q): Query<HashMap<String, String>>, req: Request) -> Response {
    let trace_id = trace_of(req.headers(), None);
    let peer = req.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip());
    let here = peer.is_some_and(|ip| ip.is_loopback() || crate::control::bound().is_some_and(|b| b.ip() == ip));
    let presented = bearer(req.headers()).or_else(|| q.get("token").cloned());
    if !here {
        if let Err(f) = ctx.app.tokens.authenticate(presented.as_deref()) {
            return error_response(&RpcError::scope("gallery files", "read", &[]).with("detail", f.message()), &trace_id);
        }
    }
    let Some(rel) = gallery::draft::tree_clean(&path) else {
        return error_response(&RpcError::invalid_params(format!("{path:?} is not a file of an item")), &trace_id);
    };
    let entry = match super::super::entry(&id).await {
        Ok(e) => e,
        Err(e) => return error_response(&e, &trace_id),
    };
    let Some(file) = entry.dir().map(|d| d.join(&rel)).filter(|f| f.is_file()) else {
        return error_response(&RpcError::not_found("file", &rel, &[]).with("item", id), &trace_id);
    };
    match tokio::fs::read(&file).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, mime(&rel)), (header::CACHE_CONTROL, "no-cache")], bytes).into_response(),
        Err(e) => error_response(&RpcError::internal(format!("reading {}: {e}", file.display())), &trace_id),
    }
}

fn mime(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "webm" => "video/webm",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
}

/// `POST /api/v1/gallery/upload?name=<file name>`: the body is a file a
/// person picked or dropped, checked and taken in like `gallery.import`.
pub async fn import_upload(State(ctx): State<Ctx>, Query(q): Query<HashMap<String, String>>, req: Request) -> Response {
    let trace_id = trace_of(req.headers(), None);
    let token = match ctx.app.tokens.authenticate(bearer(req.headers()).as_deref()) {
        Ok(t) => t,
        Err(f) => return error_response(&RpcError::scope("gallery.import", "operate", &[]).with("detail", f.message()), &trace_id),
    };
    if !token.has(Scope::Operate) {
        return error_response(&RpcError::scope("gallery.import", "operate", &token.scope_names()), &trace_id);
    }
    if !ctx.app.library.cfg().allow_upload {
        return error_response(&RpcError::not_in_state("this mixer does not take new files (media.allow_upload is off). Turn on Allow uploads in Settings."), &trace_id);
    }
    let name = q.get("name").cloned().unwrap_or_else(|| "upload".into());
    let limit = ctx.app.library.cfg().max_upload_bytes;
    let bytes = match axum::body::to_bytes(req.into_body(), limit).await {
        Ok(b) => b.to_vec(),
        Err(e) => return error_response(&RpcError::invalid_params(format!("{name} could not be read, or is over the {limit} byte upload limit: {e}")), &trace_id),
    };
    let replace = q.get("replace").is_some_and(|v| v == "true");
    let found = match tokio::task::spawn_blocking(move || bundle::read_bytes(&name, bytes)).await {
        Ok(f) => f,
        Err(e) => return error_response(&RpcError::internal(format!("reading the upload: {e}")), &trace_id),
    };
    match super::take_in(found, replace).await {
        Ok(answer) => (StatusCode::OK, axum::Json(answer)).into_response(),
        Err(e) => error_response(&e, &trace_id),
    }
}

/// `GET /api/v1/gallery/exports/{file}`: a zip `gallery.export` wrote.
pub async fn download_export(State(ctx): State<Ctx>, Path(file): Path<String>, Query(q): Query<HashMap<String, String>>, req: Request<Body>) -> Response {
    let trace_id = trace_of(req.headers(), None);
    let presented = bearer(req.headers()).or_else(|| q.get("token").cloned());
    if let Err(f) = ctx.app.tokens.authenticate(presented.as_deref()) {
        return error_response(&RpcError::scope("gallery.export", "read", &[]).with("detail", f.message()), &trace_id);
    }
    let plain = file.ends_with(".zip") && !file.contains(['/', '\\']) && !file.starts_with('.');
    let path = gallery::dir().join("exports").join(&file);
    match (plain, tokio::fs::read(&path).await) {
        (true, Ok(bytes)) => (
            [(header::CONTENT_TYPE, "application/zip".to_string()), (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{file}\""))],
            bytes,
        )
            .into_response(),
        _ => error_response(&RpcError::not_found("export", &file, &[]).with("next", "gallery.export writes one"), &trace_id),
    }
}
