//! `/whep/<output>`: WebRTC playback from a `whep/output`, on the control
//! port.
//!
//! | Route | Answer |
//! |---|---|
//! | `POST /whep/{output}` | `201` with the SDP answer and the session's `Location` |
//! | `POST /whep/program` | the same, from the first WHEP output |
//! | `DELETE /whep/{output}/{session}` | `200`, the viewer's session ends |
//! | `PATCH /whep/{output}/{session}` | `405`: every candidate is in the answer, no trickle |
//!
//! A viewer is let in by the output's viewer key (`?key=`, or the bearer
//! token) or by a control token with the read scope. The work is
//! `godwinmix_core::whep`; this is the HTTP and nothing else.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;
use godwinmix_core::whep;
use serde_json::json;

use crate::control::{presented_token, Ctx};

pub fn router(ctx: Ctx) -> Router<Ctx> {
    Router::new()
        .route("/whep/{target}/{session}", axum::routing::delete(end).patch(no_trickle))
        .with_state(ctx)
}

fn refuse(status: u16, message: &str) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST);
    (code, axum::Json(json!({ "error": message }))).into_response()
}

/// The viewer key if one was presented, else the control token with `read`.
fn let_in(ctx: &Ctx, target: &str, headers: &HeaderMap, query: &HashMap<String, String>) -> Result<(), Response> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(str::trim);
    let key = query.get("key").map(String::as_str).or(bearer).unwrap_or("");
    if whep::admits(target, key) {
        return Ok(());
    }
    let uri: Uri = "/".parse().expect("a static uri");
    let token = ctx
        .app
        .tokens
        .authenticate(presented_token(&Method::POST, headers, &uri).as_deref())
        .map_err(|reason| refuse(401, &format!("{} Or add ?key=<the output's viewer key>, which the output's status shows.", reason.message())))?;
    if !token.has(godwinmix_protocol::scope::Scope::Read) {
        return Err(refuse(403, "this token does not carry the read scope, which watching needs"));
    }
    Ok(())
}

pub async fn offer(
    State(ctx): State<Ctx>,
    Path(target): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let sdp_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !sdp_type.starts_with("application/sdp") {
        return refuse(415, "a WHEP offer is the SDP itself, sent with Content-Type: application/sdp.");
    }
    if !godwinmix_core::preview::whep::available() {
        return refuse(501, &godwinmix_core::preview::whep::missing_message());
    }
    if let Err(r) = let_in(&ctx, &target, &headers, &query) {
        return r;
    }
    let who = target.clone();
    match tokio::task::spawn_blocking(move || whep::offer(&who, &body)).await {
        Ok(Ok((output, session, answer))) => {
            let location = format!("/whep/{output}/{session}");
            let headers = [(header::CONTENT_TYPE, "application/sdp".to_string()), (header::LOCATION, location)];
            (StatusCode::CREATED, headers, answer).into_response()
        }
        Ok(Err(r)) => refuse(r.status, &r.message),
        Err(e) => refuse(500, &format!("the WHEP offer was not handled: {e}")),
    }
}

async fn end(
    State(ctx): State<Ctx>,
    Path((target, session)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = let_in(&ctx, &target, &headers, &query) {
        return r;
    }
    match tokio::task::spawn_blocking(move || whep::end(&target, &session)).await {
        Ok(true) => StatusCode::OK.into_response(),
        _ => refuse(404, "no WHEP session by that name; it may have ended already."),
    }
}

async fn no_trickle() -> Response {
    let why = "this WHEP endpoint puts every candidate in its answer and takes none later. Send the offer once gathering is complete.";
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "DELETE")], why).into_response()
}
