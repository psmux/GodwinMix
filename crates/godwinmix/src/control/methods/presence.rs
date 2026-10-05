//! `presence.list` and `presence.set`: who else is operating this mixer.
//!
//! The list is the `/rpc` connections, each under its own client id, with the
//! device its browser said it was and the scene it says it is editing. The
//! work is in `control/presence.rs`; this is the table.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::presence::{PresenceList, PresenceSetRequest};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "presence.list",
            Scope::Read,
            "Every client connected to /rpc: its client id, token, label, device, the \
             scene it says it is editing and when it connected. `you` marks the caller.",
            handler(|call: Call, _| async move {
                body(call.app.presence.list(Some(&call.client)))
            }),
        )
        .result(schema_of::<PresenceList>),
    );

    reg.register(
        MethodDef::new(
            "presence.set",
            Scope::Read,
            "Tell everybody else which scene this connection is editing, or none, and \
             optionally a name for the device. Changes nothing on air.",
            handler(set),
        )
        .params(schema_of::<PresenceSetRequest>)
        .result(schema_of::<PresenceList>)
        .mutating(false),
    );
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PresenceSetRequest = call.params(&params)?;
    // Filed by id, whatever the caller named it by, so two clients that call
    // one scene by its name and by its id are seen to be in the same place.
    let scene = match req.scene.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(which) => Some(
            call.app
                .scenes
                .scene(which)
                .map_err(|e| super::scenes::scene_error(&call, e))?
                .id
                .to_string(),
        ),
    };
    if !call.app.presence.set(&call.client, scene, req.label) {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!(
                "no /rpc connection is called {:?}, so there is nobody to describe. \
                 presence.set describes the connection it is sent on: send it over your \
                 /rpc socket, or pass that socket's client_id.",
                call.client
            ),
        )
        .with("client_id", call.client.clone())
        .with("method", call.method));
    }
    body(call.app.presence.list(Some(&call.client)))
}
