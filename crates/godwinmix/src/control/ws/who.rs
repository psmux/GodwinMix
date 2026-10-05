//! Which device a `/rpc` connection is, and telling it who else is here.
//!
//! Every connection has a client id of its own, `<token id>.<name>`, so two
//! phones on the default token are two clients: each suppresses the echo of
//! its own edits and of nobody else's, and each has its own undo stack. The
//! name is `?client_id=` on the URL when the page chose one (a phone that
//! reconnects keeps its stack that way), or one the core makes up.

use super::Connection;
use crate::control::AppState;
use axum::http::{header, HeaderMap, Uri};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::presence::{valid_client_name, MAX_CLIENT_NAME};
use godwinmix_protocol::rpc;
use serde_json::json;

/// What the upgrade request said about the device on the other end.
pub struct Who {
    /// The connection's own name, the part after the token id.
    pub name: String,
    pub user_agent: Option<String>,
}

impl Who {
    /// Read `?client_id=` and the User-Agent. A name that is not a slug is
    /// refused rather than quietly replaced, so a page that meant to keep its
    /// undo stack across a reconnect is told why it would not.
    pub fn of(app: &AppState, headers: &HeaderMap, uri: &Uri) -> Result<Who, RpcError> {
        let asked = uri.query().unwrap_or("").split('&').find_map(|pair| pair.strip_prefix("client_id="));
        let name = match asked.filter(|n| !n.is_empty()) {
            None => app.presence.fresh_name(),
            Some(n) if valid_client_name(n) => n.to_string(),
            Some(n) => {
                return Err(RpcError::invalid_params(format!(
                    "client_id {n:?} on the /rpc URL is not a name this core accepts. Use up to \
                     {MAX_CLIENT_NAME} lower case letters, digits and dashes, or leave it off \
                     and the core will name the connection."
                ))
                .with("client_id", n)
                .with("pattern", "^[a-z0-9-]+$"))
            }
        };
        let user_agent =
            headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(str::to_string);
        Ok(Who { name, user_agent })
    }
}

impl Connection {
    pub(super) fn wants_presence(&self) -> bool {
        self.sub.as_ref().is_some_and(|s| s.wants("presence.changed"))
    }

    /// `event/presence.changed`, with the whole list, to a client that asked.
    /// The list is built here and only here, so a core nobody is watching
    /// presence on never builds one.
    pub(super) async fn send_presence(&mut self) -> Result<(), ()> {
        if !self.wants_presence() {
            return Ok(());
        }
        let list = self.ctx.app.presence.list(Some(&self.client_id));
        let mut value = serde_json::to_value(list).map_err(|_| ())?;
        if let Some(map) = value.as_object_mut() {
            map.insert("seq".into(), json!(self.seq));
        }
        self.send(rpc::notification("event/presence.changed", value)).await
    }
}
