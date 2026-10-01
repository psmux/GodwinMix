//! Who may read an HLS output, and who a request is for counting viewers.

use super::{refuse, Door};
use crate::control::presented_token;
use axum::extract::{ConnectInfo, Request};
use axum::http::{header, StatusCode};
use axum::response::Response;
use godwinmix_core::hls::Stream;
use godwinmix_protocol::scope::Scope;
use serde_json::json;
use std::net::SocketAddr;

/// A request that was let in.
pub struct Viewer {
    /// `key`, `token` and `v` exactly as they came, for every URI a
    /// playlist hands this player.
    pub query: String,
    /// Who this is, for the viewer count.
    pub who: String,
    has_id: bool,
}

impl Viewer {
    /// Give a player that opened the multivariant playlist an id of its own,
    /// carried on every URI after, so two players behind one address count
    /// as two.
    pub fn ensure_id(&mut self) {
        if self.has_id {
            return;
        }
        if let Ok(id) = godwinmix_core::secrets::random_key(10) {
            if !self.query.is_empty() {
                self.query.push('&');
            }
            self.query.push_str(&format!("v={id}"));
            self.who = format!("v:{id}");
            self.has_id = true;
        }
    }
}

/// The raw `name=value` pairs of the query, undecoded.
pub fn pairs(req: &Request) -> Vec<(&str, &str)> {
    req.uri()
        .query()
        .unwrap_or("")
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| p.split_once('=').unwrap_or((p, "")))
        .collect()
}

pub fn value<'a>(pairs: &[(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// Let `req` read `stream`, or say why not. `show` is carried with the
/// key, so on a station every URI a playlist hands out reaches the same
/// show's output.
pub fn admit(door: &impl Door, stream: &Stream, req: &Request) -> Result<Viewer, Box<Response>> {
    let pairs = pairs(req);
    let key = value(&pairs, "key");
    if !key.is_some_and(|k| stream.admits(k)) {
        check_token(door, stream, req, key.is_some())?;
    }
    let query = pairs
        .iter()
        .filter(|(k, _)| matches!(*k, "show" | "key" | "token" | "v"))
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let id = value(&pairs, "v").filter(|v| !v.is_empty() && v.len() <= 32);
    let who = match id {
        Some(v) => format!("v:{v}"),
        None => {
            let peer = req.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip().to_string()).unwrap_or_default();
            let agent = req.headers().get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
            format!("{peer} {agent}")
        }
    };
    Ok(Viewer { query, who, has_id: id.is_some() })
}

fn check_token(door: &impl Door, stream: &Stream, req: &Request, had_key: bool) -> Result<(), Box<Response>> {
    let presented = presented_token(req.method(), req.headers(), req.uri());
    let link = stream.master_url_path();
    let token = door.tokens().authenticate(presented.as_deref()).map_err(|reason| {
        let message = if had_key {
            format!(
                "That viewer key is not this output's. Open the link the output shows ({link} with its key), \
                 which changes when the output is added again."
            )
        } else {
            format!(
                "{}. A player opens this with the output's viewer key, the `playback.master_url_path` \
                 that output.list shows for `{}`.",
                reason.message(),
                stream.id
            )
        };
        Box::new(refuse(StatusCode::UNAUTHORIZED, message, json!({ "output": stream.id, "needs": "key" })))
    })?;
    if !token.has(Scope::Read) {
        return Err(Box::new(refuse(
            StatusCode::FORBIDDEN,
            "this token does not carry the read scope, which watching an HLS output needs. Use the output's \
             viewer link instead."
                .into(),
            json!({ "output": stream.id, "needs": "read" }),
        )));
    }
    Ok(())
}
