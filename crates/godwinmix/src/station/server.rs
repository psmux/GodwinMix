//! The station's control port: the page, `/rpc`, `/api/v1`, WHIP for its
//! channels, and every other path relayed to the show it names.

use super::methods;
use super::relay::{http, pipe, rpc};
use super::state::Station;
use crate::control::rest::{self, Route};
use axum::extract::{FromRequestParts, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use serde_json::Value;
use std::sync::{Arc, OnceLock};
use tower_http::cors::CorsLayer;

pub fn router(st: Arc<Station>) -> Router {
    let whip = st.channels.get().map(|c| crate::control::whip::router_for(c.clone()));
    let mut router = Router::new()
        .merge(crate::ui::router())
        .route("/rpc", get(rpc_upgrade))
        .route("/api/v1/{*rest}", any(api))
        .fallback(any(relay))
        .with_state(st);
    if let Some(whip) = whip {
        router = router.merge(whip);
    }
    router.layer(CorsLayer::permissive())
}

async fn rpc_upgrade(State(st): State<Arc<Station>>, req: Request) -> Response {
    let (mut parts, _) = req.into_parts();
    match axum::extract::ws::WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(ws) => rpc::upgrade(st, ws, parts.uri, parts.headers).await,
        Err(e) => e.into_response(),
    }
}

/// Anything the station does not serve itself: a WebSocket is piped, the
/// rest is forwarded.
async fn relay(State(st): State<Arc<Station>>, req: Request) -> Response {
    let upgrade = req.headers().get("upgrade").and_then(|v| v.to_str().ok()).is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    if !upgrade {
        return http::forward(st, req).await;
    }
    let (mut parts, _) = req.into_parts();
    match axum::extract::ws::WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(ws) => pipe::upgrade(st, ws, parts.uri, parts.headers).await,
        Err(e) => e.into_response(),
    }
}

fn routes() -> &'static [Route] {
    static ROUTES: OnceLock<Vec<Route>> = OnceLock::new();
    ROUTES.get_or_init(|| rest::routes(methods::registry()))
}

/// `/api/v1`: a station method is answered here, anything else goes to the
/// show.
async fn api(State(st): State<Arc<Station>>, req: Request) -> Response {
    let resolved = rest::resolve(routes(), req.method(), req.uri().path());
    let Ok((route, captures)) = resolved else { return http::forward(st, req).await };
    let query = req.uri().query().unwrap_or_default().to_string();
    let peek = rest::params_from(Value::Null, &query, captures.clone());
    if !methods::answers(route.method, &peek) {
        return http::forward(st, req).await;
    }
    let presented = crate::control::presented_token(req.method(), req.headers(), req.uri());
    let token = match st.tokens.authenticate(presented.as_deref()) {
        Ok(t) => t,
        Err(f) => {
            let e = godwinmix_protocol::error::RpcError::new(godwinmix_protocol::ErrorCode::Scope, format!("{}. Send it as `Authorization: Bearer <token>`.", f.message()));
            return (StatusCode::UNAUTHORIZED, Json(e.body(&godwinmix_protocol::trace::new_id()))).into_response();
        }
    };
    let body = match axum::body::to_bytes(req.into_body(), 1 << 20).await {
        Ok(b) if b.is_empty() => Value::Null,
        Ok(b) => match serde_json::from_slice(&b) {
            Ok(v) => v,
            Err(e) => return http::refusal(&godwinmix_protocol::error::RpcError::invalid_params(format!("the body is not JSON: {e}"))),
        },
        Err(e) => return http::refusal(&godwinmix_protocol::error::RpcError::invalid_params(format!("could not read the body: {e}"))),
    };
    let query = super::relay::without_show(Some(&query)).unwrap_or_default();
    let params = rest::params_from(body, &query, captures);
    match methods::call(&st, &token, route.method, params).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => http::refusal(&e),
    }
}
