//! `token.create`, `token.list` and `token.revoke`, the same for a core on its
//! own and for a station, which answers them itself so the file has one
//! writer.

use godwinmix_protocol::devices::{DeviceRegistry, TokenCreateRequest, TokenList, TokenRevokeRequest, TokenRevoked};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::scope::{Scope, Tokens};
use serde_json::Value;
use std::sync::Arc;

use crate::channels::keys;

/// Longest label kept. A phone's name, not a paragraph.
const LABEL_MAX: usize = 64;

/// Answer one of the three. The work touches a small file, so it runs on the
/// blocking pool rather than on the thread that reads the socket.
pub async fn call(tokens: &Tokens, method: &str, params: Value) -> Result<Value, RpcError> {
    let registry = registry(tokens)?;
    let taken = {
        let tokens = tokens.clone();
        move |id: &str| tokens.id_in_use(id)
    };
    let method = method.to_string();
    tokio::task::spawn_blocking(move || match method.as_str() {
        "token.create" => {
            let req: TokenCreateRequest = read(&method, params)?;
            body(registry.create(req, &taken)?)
        }
        "token.list" => body(TokenList { tokens: registry.list() }),
        "token.revoke" => {
            let req: TokenRevokeRequest = read(&method, params)?;
            body(TokenRevoked { revoked: registry.revoke(&req.id)? })
        }
        other => Err(RpcError::internal(format!("{other} is not a device token method"))),
    })
    .await
    .map_err(|e| RpcError::internal(format!("the device token call stopped: {e}. Try it again.")))?
}

/// The registry, or why there is none worth having. An open core is refused:
/// anyone who reaches it is already admin, so a device token there would look
/// like protection and be none.
fn registry(tokens: &Tokens) -> Result<Arc<dyn DeviceRegistry>, RpcError> {
    if tokens.is_open() {
        return Err(RpcError::new(
            ErrorCode::NotInState,
            "this mixer has no control token, so anyone who can reach it is already admin and a \
             device token would protect nothing. Set one first: GODWINMIX_TOKEN in the \
             environment or control.token in the config, then restart the mixer.",
        )
        .with("open", true));
    }
    tokens.devices().cloned().ok_or_else(|| {
        RpcError::not_in_state("this core keeps no device tokens. Call it through the station, or start it with a config file.")
    })
}

fn read<T: serde::de::DeserializeOwned>(method: &str, params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|e| {
        RpcError::invalid_params(format!("{method} could not read its params: {e}. Call core.api for the schema."))
            .with("method", method)
    })
}

fn body<T: serde::Serialize>(value: T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::internal(format!("encoding the answer: {e}")))
}

/// A `token.create` request, checked.
pub(super) struct Wanted {
    pub label: String,
    pub scope: Scope,
    id: Option<String>,
}

impl Wanted {
    pub fn check(req: TokenCreateRequest) -> Result<Self, RpcError> {
        if req.scope == Scope::Plugin {
            return Err(RpcError::invalid_params(
                "a device token cannot have the plugin scope, which belongs to a plugin's own \
                 token. Use read, operate or admin.",
            )
            .with("field", "scope"));
        }
        let label = req.label.as_deref().map(str::trim).filter(|l| !l.is_empty()).unwrap_or("Phone");
        let label: String = label.chars().take(LABEL_MAX).collect();
        if let Some(id) = req.id.as_deref() {
            if keys::slug(id) != id || id.is_empty() {
                return Err(RpcError::invalid_params(format!(
                    "'{id}' is not a slug. Use lower case letters, digits and dashes, starting with \
                     a letter, such as '{}', or leave id out to have one made from the label.",
                    fallback(&keys::slug(id))
                ))
                .with("field", "id"));
            }
        }
        Ok(Wanted { label, scope: req.scope, id: req.id })
    }

    /// The id: the one asked for, which must be free, or one made from the
    /// label that is.
    pub fn id(&self, taken: &dyn Fn(&str) -> bool) -> Result<String, RpcError> {
        match &self.id {
            Some(id) if taken(id) => Err(RpcError::new(
                ErrorCode::InvalidParams,
                format!("a token called '{id}' already exists. Pick another id, or leave id out to have a free one made."),
            )
            .with("field", "id")
            .with("id", id.as_str())),
            Some(id) => Ok(id.clone()),
            None => Ok(keys::free(&fallback(&keys::slug(&self.label)), taken)),
        }
    }
}

fn fallback(slug: &str) -> String {
    if slug.is_empty() {
        "device".into()
    } else {
        slug.to_string()
    }
}
