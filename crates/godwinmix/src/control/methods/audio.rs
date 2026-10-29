//! `audio.master.set`, `audio.bus.*`, `audio.send.set`, `audio.cue.set` and
//! `audio.device.list`: the desk beyond one fader per source.
//!
//! The mixer owns the buses; everything here turns a call into one
//! `AudioCall` and the refusal back into an error that names the next step.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::buses::{AudioCall, Refusal};
use godwinmix_protocol::buses::*;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("audio.master.set", Scope::Operate, "Move the programme's master fader or mute. Only what is named moves.", handler(master))
            .params(schema_of::<MasterSetRequest>)
            .result(schema_of::<MasterConfig>)
            .tool(
                "set_master_audio",
                Tier::Search,
                "Set the level of the whole programme mix, or mute it. `gain` is 0.0 silent, 1.0 \
                 unity, up to 10.0, and out of range is refused. Buses and the mix minus are fed \
                 ahead of the master, so a guest still hears the show while the programme is muted.",
            ),
    );
    reg.register(
        MethodDef::new("audio.bus.list", Scope::Read, "The programme master, every bus with its sends, and the sources marked cue.", handler(list))
            .result(schema_of::<AudioStatus>)
            .tool(
                "list_audio_buses",
                Tier::Search,
                "Every audio bus: its kind (sends, minus or cue), its master, its sends per \
                 source, the outputs that take their sound from it, the sound card it plays on, \
                 and the /pcm/ path to hear it. Also the programme master and the cue marks.",
            ),
    );
    reg.register(
        MethodDef::new("audio.bus.add", Scope::Operate, "Add a bus beside the programme mix, live. Nothing is built for buses until the first one.", handler(add))
            .params(schema_of::<BusConfig>)
            .result(schema_of::<BusStatus>)
            .not_idempotent()
            .tool(
                "add_audio_bus",
                Tier::Search,
                "Add a mix of its own beside the programme. `id` is a slug. With `sends` \
                 ({\"cam1\": 0.5}) it carries those sources at those levels. With `minus: \
                 \"guest-1\"` it is the programme without that source, for a remote guest to \
                 hear everyone but themselves. With `cue: true` it carries the sources marked \
                 with set_audio_cue, and the programme while none is. `device` plays it on a \
                 sound card here (see audio.device.list). An output takes it as its sound with \
                 `audio = \"<bus id>\"`.",
            ),
    );
    reg.register(
        MethodDef::new("audio.bus.set", Scope::Operate, "Change a bus in place: its name, master, mute, the source a mix minus leaves out, or its sound card.", handler(set))
            .params(schema_of::<BusSetRequest>)
            .result(schema_of::<BusStatus>)
            .tool(
                "set_audio_bus",
                Tier::Search,
                "Change one bus. Only the keys given change. `gain` and `muted` are the bus \
                 master. `minus` names the source a mix minus leaves out. `device` plays it on a \
                 sound card, and an empty `device` stops that.",
            ),
    );
    reg.register(
        MethodDef::new("audio.bus.remove", Scope::Operate, "Take a bus out, live. Refused while an output takes its sound from it.", handler(remove))
            .params(schema_of::<BusIdRequest>)
            .result(schema_of::<BusIdRequest>)
            .destructive(),
    );
    reg.register(
        MethodDef::new("audio.send.set", Scope::Operate, "Set how much of one source goes to one bus. 0 takes it off the bus.", handler(send))
            .params(schema_of::<SendSetRequest>)
            .result(schema_of::<BusStatus>)
            .tool(
                "set_audio_send",
                Tier::Search,
                "Send a source to a bus at a level: 0.0 off, 1.0 unity, up to 10.0. The send is \
                 after the source's own fader and follows its mute. Refused on a mix minus or \
                 cue bus, which follow the programme instead.",
            ),
    );
    reg.register(
        MethodDef::new("audio.cue.set", Scope::Operate, "Mark or clear a source for the cue bus. `solo` clears every other mark first.", handler(cue))
            .params(schema_of::<CueSetRequest>)
            .result(schema_of::<AudioStatus>)
            .tool(
                "set_audio_cue",
                Tier::Search,
                "Listen to a source on the cue bus without touching what is on air: `cue: true` \
                 adds it, `cue: false` takes it off, and `solo: true` leaves it the only one. A \
                 muted source is heard on cue too. With nothing marked the cue bus carries the \
                 programme. Marks are not kept across a restart.",
            ),
    );
    reg.register(
        MethodDef::new("audio.device.list", Scope::Read, "The sound outputs on the mixer's machine that a bus can play on.", handler(devices))
            .result(schema_of::<AudioDeviceList>),
    );
}

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

async fn master(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: MasterSetRequest = call.params(&params)?;
    body(desk(&call, AudioCall::Master(req)).await?.master)
}

async fn list(call: Call, _params: Value) -> Result<Value, RpcError> {
    body(desk(&call, AudioCall::List).await?)
}

async fn add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: BusConfig = call.params(&params)?;
    let id = req.id.clone();
    one_bus(desk(&call, AudioCall::Add(req)).await?, &id)
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: BusSetRequest = call.params(&params)?;
    let id = req.id.clone();
    one_bus(desk(&call, AudioCall::Set(req)).await?, &id)
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: BusIdRequest = call.params(&params)?;
    desk(&call, AudioCall::Remove(req.id.clone())).await?;
    body(req)
}

async fn send(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SendSetRequest = call.params(&params)?;
    let id = req.bus.clone();
    one_bus(desk(&call, AudioCall::Send(req)).await?, &id)
}

async fn cue(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: CueSetRequest = call.params(&params)?;
    body(desk(&call, AudioCall::Cue(req)).await?)
}

/// Listed off the async workers: a device monitor can take seconds to start.
async fn devices(_call: Call, _params: Value) -> Result<Value, RpcError> {
    let found = tokio::task::spawn_blocking(|| godwinmix_core::buses::device::list().into_iter().map(|(d, _)| d).collect())
        .await
        .map_err(|e| RpcError::internal(format!("listing the sound outputs stopped: {e}")))?;
    body(AudioDeviceList { devices: found })
}
