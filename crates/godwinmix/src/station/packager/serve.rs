//! The packager's loopback port: the control port's own `/hls` handlers
//! over this process's outputs, and the two routes the station hands the
//! outputs over and reads them back on.

use super::outputs::Outputs;
use super::wire::{Want, OUTPUTS, PEER_HEADER, SHOW_HEADER};
use crate::control::hls::{refuse, Door};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use godwinmix_core::hls::Stream;
use godwinmix_protocol::scope::{constant_time_eq, Token, Tokens};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
pub struct PackagerDoor {
    outputs: Arc<Outputs>,
    tokens: Arc<Tokens>,
    secret: Arc<str>,
}

impl PackagerDoor {
    pub fn new(outputs: Arc<Outputs>, secret: &str) -> PackagerDoor {
        let tokens = Arc::new(Tokens::new(vec![Token::legacy(secret)], false));
        PackagerDoor { outputs, tokens, secret: secret.into() }
    }

    fn station(&self, headers: &HeaderMap) -> bool {
        let presented = crate::control::rest::bearer(headers).unwrap_or_default();
        constant_time_eq(presented.as_bytes(), self.secret.as_bytes())
    }
}

impl Door for PackagerDoor {
    fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    fn find(&self, output: &str, req: &Request) -> Result<Arc<Stream>, Box<Response>> {
        let show = req.headers().get(SHOW_HEADER).and_then(|v| v.to_str().ok()).unwrap_or_default();
        self.outputs.stream(show, output).ok_or_else(|| {
            let have = self.outputs.ids(show);
            let message = format!("Show {show} has no HLS output `{output}` in the packager now. It serves {}.", have.join(", "));
            Box::new(refuse(StatusCode::NOT_FOUND, message, json!({ "show": show, "output": output, "outputs": have })))
        })
    }
}

pub fn router(door: PackagerDoor) -> Router {
    let outputs = Router::new().route(OUTPUTS, get(reports).put(take)).with_state(door.clone());
    crate::control::hls::router(door.clone()).with_state(door).merge(outputs).layer(middleware::from_fn(peer))
}

/// A request the station forwarded comes from the station's loopback
/// socket; the player's own address, which the viewer count goes by, is in
/// [`PEER_HEADER`].
async fn peer(mut req: Request, next: Next) -> Response {
    let named = req.headers().get(PEER_HEADER).and_then(|v| v.to_str().ok()?.parse::<SocketAddr>().ok());
    if let Some(addr) = named {
        req.extensions_mut().insert(ConnectInfo(addr));
    }
    next.run(req).await
}

fn not_the_station() -> Response {
    let message = "this port is the station's HLS packager and answers the station alone".to_string();
    refuse(StatusCode::UNAUTHORIZED, message, json!({ "needs": "the packager's secret" }))
}

async fn take(State(door): State<PackagerDoor>, headers: HeaderMap, Json(wanted): Json<Vec<Want>>) -> Response {
    if !door.station(&headers) {
        return not_the_station();
    }
    door.outputs.apply(wanted);
    StatusCode::NO_CONTENT.into_response()
}

async fn reports(State(door): State<PackagerDoor>, headers: HeaderMap) -> Response {
    if !door.station(&headers) {
        return not_the_station();
    }
    Json(door.outputs.reports()).into_response()
}
