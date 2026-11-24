//! `user.*` and `account.*`, the same for a core on its own and for a
//! station, which answers them itself.

use godwinmix_protocol::accounts::{
    AccountPasswordRequest, Role, SessionList, SignedOut, UserAddRequest, UserList, UserNameRequest, UserPasswordRequest,
    UserSessionsRequest, UserSignOutRequest,
};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::scope::{Token, Tokens};
use serde_json::Value;
use std::sync::Arc;

use super::{Accounts, SECRET_PREFIX};

/// Ids no account may take: what an open port, the single token and a
/// station's own credential are recorded as.
const RESERVED: &[&str] = &["open", "default", "station"];

/// Answer one. The work may hash a password, so it runs on the blocking pool
/// rather than on the thread that reads the socket.
pub async fn call(tokens: &Arc<Tokens>, token: &Token, method: &str, params: Value) -> Result<Value, RpcError> {
    if method == "account.get" {
        return body(super::http::info(tokens, Some(token)));
    }
    if super::of(tokens).is_none() {
        return Err(RpcError::not_in_state("this core keeps no accounts. Call it through the station, or start it with a config file."));
    }
    let (tokens, token, method) = (tokens.clone(), token.clone(), method.to_string());
    tokio::task::spawn_blocking(move || {
        let store = super::of(&tokens).ok_or_else(|| RpcError::internal("the accounts went away"))?;
        answer(&tokens, store, &token, &method, params)
    })
    .await
    .map_err(|e| RpcError::internal(format!("the account call stopped: {e}. Try it again.")))?
}

fn answer(tokens: &Tokens, store: &Accounts, token: &Token, method: &str, params: Value) -> Result<Value, RpcError> {
    let current = token.secret.strip_prefix(SECRET_PREFIX);
    match method {
        "user.list" => body(UserList { users: store.list() }),
        "user.add" => {
            let req: UserAddRequest = read(method, params)?;
            if !tokens.has_accounts() && req.role != Role::Admin {
                return Err(RpcError::new(
                    ErrorCode::InvalidParams,
                    "the first account has to be an admin, or nobody could manage the users from a page. Add it with role admin; the next ones can be anything.",
                )
                .with("field", "role"));
            }
            let in_use = |id: &str| taken(tokens, id);
            body(store.add(req, &in_use)?)
        }
        "user.update" => body(store.update(read(method, params)?)?),
        "user.reset_password" => {
            let req: UserPasswordRequest = read(method, params)?;
            // An admin resetting their own password keeps the page they are on.
            let keep = current.filter(|_| req.name.trim().eq_ignore_ascii_case(&token.id));
            body(store.reset_password(&req.name, &req.password, keep)?)
        }
        "user.remove" => body(store.remove(&read::<UserNameRequest>(method, params)?.name)?),
        "user.sessions" => {
            let req: UserSessionsRequest = read(method, params)?;
            body(SessionList { sessions: store.sessions(req.name.as_deref(), current) })
        }
        "user.sign_out" => {
            let req: UserSignOutRequest = read(method, params)?;
            body(SignedOut { sessions_ended: store.sign_out(req.name.as_deref(), req.session.as_deref())? })
        }
        "account.change_password" => {
            let req: AccountPasswordRequest = read(method, params)?;
            body(SignedOut { sessions_ended: store.change_own(token, &req.current, &req.password)? })
        }
        other => Err(RpcError::internal(format!("{other} is not an account method"))),
    }
}

/// Whether `id` is already somebody's on this mixer: a configured or minted
/// token, a device token, or one of the reserved names.
fn taken(tokens: &Tokens, id: &str) -> bool {
    RESERVED.contains(&id) || tokens.id_in_use(id) || tokens.devices().is_some_and(|d| d.list().iter().any(|t| t.id == id))
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
