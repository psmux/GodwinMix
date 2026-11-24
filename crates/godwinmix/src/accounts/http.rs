//! `/api/v1/auth/*`: the three routes a sign in page needs.
//!
//! * `GET /api/v1/auth/status` says whether this mixer has accounts and who
//!   the request is signed in as. Open, because the page asks it before it
//!   has anything to sign in with.
//! * `POST /api/v1/auth/login` takes `{"username", "password"}` and answers
//!   with the session cookie.
//! * `POST /api/v1/auth/logout` ends the session the request carries and
//!   clears the cookie.
//!
//! They sit outside the method table because the table's door is a token
//! and these are how a browser gets one. Everything after the sign in is
//! the table, with the session standing in for the token.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use godwinmix_protocol::accounts::AccountInfo;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::scope::Tokens;
use serde::Deserialize;

use super::{cookie, Refused, SECRET_PREFIX};

/// The three routes, for a core's port or a station's.
pub fn router(tokens: Arc<Tokens>) -> Router {
    Router::new()
        .route("/api/v1/auth/status", get(status))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .with_state(tokens)
}

/// Who a request is, in a person's terms. Also `account.get`.
pub fn info(tokens: &Tokens, token: Option<&godwinmix_protocol::scope::Token>) -> AccountInfo {
    let accounts = tokens.has_accounts();
    let Some(token) = token else { return AccountInfo { accounts, user: None, role: None, via: "none".into() } };
    if token.secret.starts_with(SECRET_PREFIX) {
        let role = super::of(tokens).and_then(|a| a.role_of(&token.id));
        return AccountInfo { accounts, user: Some(token.id.clone()), role, via: "session".into() };
    }
    let via = if tokens.is_open() { "open" } else { "token" };
    AccountInfo { accounts, user: None, role: None, via: via.into() }
}

async fn status(State(tokens): State<Arc<Tokens>>, headers: HeaderMap) -> Response {
    let presented = crate::control::rest::bearer(&headers);
    let token = tokens.authenticate(presented.as_deref()).ok();
    let mut answer = Json(info(&tokens, token.as_ref())).into_response();
    answer.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
    answer
}

#[derive(Deserialize)]
struct Login {
    username: String,
    password: String,
}

fn refusal(status: StatusCode, e: RpcError) -> Response {
    (status, Json(e.body(&godwinmix_protocol::trace::new_id()))).into_response()
}

async fn login(State(tokens): State<Arc<Tokens>>, req: Request) -> Response {
    let address = req.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip().to_string()).unwrap_or_default();
    let (parts, body) = req.into_parts();
    let headers = parts.headers;
    if !cookie::same_origin(&headers) {
        return refusal(StatusCode::FORBIDDEN, RpcError::new(ErrorCode::Scope, "a sign in has to come from this mixer's own page. Open the mixer's address and sign in there."));
    }
    let Some(accounts) = tokens.accounts().cloned().filter(|_| tokens.has_accounts()) else {
        return refusal(StatusCode::CONFLICT, RpcError::not_in_state("this mixer has no accounts, so it signs people in with a token. An admin adds the first account in Mixer settings > Users, or with `godwinmix users add`.").with("accounts", false));
    };
    let bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(e) => return refusal(StatusCode::BAD_REQUEST, RpcError::invalid_params(format!("could not read the body: {e}"))),
    };
    let Ok(form) = serde_json::from_slice::<Login>(&bytes) else {
        return refusal(StatusCode::BAD_REQUEST, RpcError::invalid_params("send {\"username\": \"...\", \"password\": \"...\"} as JSON."));
    };
    let device = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    let answer = tokio::task::spawn_blocking(move || {
        let store = accounts.as_any().downcast_ref::<super::Accounts>()?;
        Some(store.sign_in(&form.username, &form.password, &address, &device))
    })
    .await;
    match answer {
        Ok(Some(Ok(done))) => {
            let mut response = Json(AccountInfo { accounts: true, user: Some(done.user), role: Some(done.role), via: "session".into() }).into_response();
            response.headers_mut().insert(header::SET_COOKIE, cookie::set(&headers, &done.secret));
            response.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
            response
        }
        Ok(Some(Err(Refused::Wait(d)))) => {
            let mut response = refusal(StatusCode::TOO_MANY_REQUESTS, RpcError::new(ErrorCode::NotInState, Refused::Wait(d).message()).with("retry_after_secs", d.as_secs()));
            if let Ok(v) = header::HeaderValue::from_str(&d.as_secs().to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, v);
            }
            response
        }
        Ok(Some(Err(Refused::NoMatch))) => refusal(StatusCode::UNAUTHORIZED, RpcError::new(ErrorCode::Scope, Refused::NoMatch.message())),
        Ok(Some(Err(Refused::Store(why)))) => refusal(StatusCode::INTERNAL_SERVER_ERROR, RpcError::internal(why)),
        Ok(None) | Err(_) => refusal(StatusCode::INTERNAL_SERVER_ERROR, RpcError::internal("the sign in stopped before it finished. Try again.")),
    }
}

async fn logout(State(tokens): State<Arc<Tokens>>, headers: HeaderMap) -> Response {
    if let (Some(secret), Some(accounts)) = (crate::control::rest::bearer(&headers), tokens.accounts().cloned()) {
        let _ = tokio::task::spawn_blocking(move || {
            if let Some(store) = accounts.as_any().downcast_ref::<super::Accounts>() {
                store.sign_out_secret(&secret);
            }
        })
        .await;
    }
    let mut response = Json(serde_json::json!({ "signed_out": true })).into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie::clear(&headers));
    response
}
