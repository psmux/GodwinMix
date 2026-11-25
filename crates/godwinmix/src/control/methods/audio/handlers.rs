//! The `audio.*` handlers: one `AudioCall` each, and the refusal turned into
//! an error that names the next step.

use super::super::body;
use crate::control::call::Call;
use godwinmix_core::buses::{AudioCall, Refusal};
use godwinmix_protocol::buses::*;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use serde_json::Value;

/// One call on the desk, and the refusal as an error a caller can act on.
async fn desk(call: &Call, audio: AudioCall) -> Result<AudioStatus, RpcError> {
    call.app.mixer.audio(audio).await.map_err(|e| call.mixer_error(e))?.map_err(refusal)
}

fn refusal(r: Refusal) -> RpcError {
    let message = r.to_string();
    match r {
        Refusal::NotFound { kind, id, known } => {
            RpcError::new(ErrorCode::NotFound, message).with("kind", kind).with("id", id).with("valid", known)
        }
        Refusal::Invalid { field, .. } => RpcError::new(ErrorCode::InvalidParams, message).with("field", field),
        Refusal::InUse { by, .. } => RpcError::not_in_state(message).with("outputs", by),
        Refusal::Failed(_) => RpcError::not_in_state(message),
    }
}

fn one_bus(status: AudioStatus, id: &str) -> Result<Value, RpcError> {
    match status.buses.into_iter().find(|b| b.config.id == id) {
        Some(b) => body(b),
        None => Err(RpcError::internal(format!("bus {id} was changed and is not in the list; call audio.bus.list"))),
    }
}

pub(super) async fn master(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: MasterSetRequest = call.params(&params)?;
    body(desk(&call, AudioCall::Master(req)).await?.master)
}

pub(super) async fn list(call: Call, _params: Value) -> Result<Value, RpcError> {
    body(desk(&call, AudioCall::List).await?)
}

pub(super) async fn add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: BusConfig = call.params(&params)?;
    let id = req.id.clone();
    one_bus(desk(&call, AudioCall::Add(req)).await?, &id)
}

pub(super) async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: BusSetRequest = call.params(&params)?;
    let id = req.id.clone();
    one_bus(desk(&call, AudioCall::Set(req)).await?, &id)
}

pub(super) async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: BusIdRequest = call.params(&params)?;
    desk(&call, AudioCall::Remove(req.id.clone())).await?;
    body(req)
}

pub(super) async fn send(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SendSetRequest = call.params(&params)?;
    let id = req.bus.clone();
    one_bus(desk(&call, AudioCall::Send(req)).await?, &id)
}

pub(super) async fn cue(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: CueSetRequest = call.params(&params)?;
    body(desk(&call, AudioCall::Cue(req)).await?)
}

/// Listed off the async workers: a device monitor can take seconds to start.
pub(super) async fn devices(_call: Call, _params: Value) -> Result<Value, RpcError> {
    let found = tokio::task::spawn_blocking(|| godwinmix_core::buses::device::list().into_iter().map(|(d, _)| d).collect())
        .await
        .map_err(|e| RpcError::internal(format!("listing the sound outputs stopped: {e}")))?;
    body(AudioDeviceList { devices: found })
}
