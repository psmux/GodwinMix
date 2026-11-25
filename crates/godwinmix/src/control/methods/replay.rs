//! `replay.*`: turning replay on and off, marks and events, playing an event
//! on a replay source and saving one to the media folder.
//!
//! The mixer owns the rings; everything here turns a call into a
//! `ReplayCall`, or into a call on the replay source's kind, and a refusal
//! into an error that names the next step.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::replay::{Refusal, ReplayCall};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::replay::*;
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

mod play;
mod save;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("replay.get", Scope::Read, "Whether replay is on, each angle's buffer depth and size, and every event.", handler(get))
            .result(schema_of::<ReplayStatus>)
            .tool("get_replay", Tier::Search, "Replay as it stands: the angles recorded, how many seconds and bytes each holds, the encoder each uses, and every marked event with its in and out points."),
    );
    reg.register(
        MethodDef::new("replay.start", Scope::Operate, "Record these angles, each the last `seconds`. Called again, it changes them in place.", handler(start))
            .params(schema_of::<ReplayConfig>)
            .result(schema_of::<ReplayStatus>)
            .tool(
                "start_replay",
                Tier::Search,
                "Keep the last `seconds` (default 60, up to 1800) of each angle in memory for instant replay: \
                 `angles` lists source ids, and \"programme\" for the programme itself. Memory is about the bitrate \
                 times the seconds per angle. Nothing is recorded until this is called.",
            ),
    );
    reg.register(
        MethodDef::new("replay.stop", Scope::Operate, "Stop recording, free every buffer and forget every event.", handler(stop))
            .result(schema_of::<ReplayStatus>)
            .destructive(),
    );
    reg.register(
        MethodDef::new("replay.mark", Scope::Operate, "Set an in point (now, or `seconds` ago) or an out point, or make an event of the last `seconds`.", handler(mark))
            .params(schema_of::<MarkRequest>)
            .result(schema_of::<ReplayEvent>)
            .not_idempotent()
            .tool(
                "mark_replay",
                Tier::Search,
                "Mark a moment for replay. `point: \"in\"` opens an event now (or `seconds` ago), `point: \"out\"` \
                 closes it, and `seconds` alone makes an event of the last that many seconds. Returns the event \
                 with its id, for play_replay and save_replay.",
            ),
    );
    reg.register(
        MethodDef::new("replay.event.list", Scope::Read, "Every replay event, oldest first.", handler(list))
            .result(schema_of::<EventList>),
    );
    reg.register(
        MethodDef::new("replay.event.rename", Scope::Operate, "Give a replay event a name.", handler(rename))
            .params(schema_of::<EventRenameRequest>)
            .result(schema_of::<ReplayEvent>),
    );
    reg.register(
        MethodDef::new("replay.event.delete", Scope::Operate, "Forget a replay event. The recording it pointed at is not touched.", handler(delete))
            .params(schema_of::<EventIdRequest>)
            .result(schema_of::<EventIdRequest>),
    );
    play::register(reg);
    save::register(reg);
}

/// One call on the desk, and the refusal as an error a caller can act on.
async fn desk(call: &Call, replay: ReplayCall) -> Result<(ReplayStatus, Option<ReplayEvent>), RpcError> {
    call.app.mixer.replay(replay).await.map_err(|e| call.mixer_error(e))?.map_err(refusal)
}

pub(super) fn refusal(r: Refusal) -> RpcError {
    let message = r.to_string();
    match r {
        Refusal::NotFound { kind, id, known } => {
            RpcError::new(ErrorCode::NotFound, message).with("kind", kind).with("id", id).with("valid", known)
        }
        Refusal::Invalid { field, .. } => RpcError::new(ErrorCode::InvalidParams, message).with("field", field),
        Refusal::NotInState(_) => RpcError::not_in_state(message).with("next", "replay.start"),
        Refusal::Failed(_) => RpcError::not_in_state(message),
    }
}

fn event_of(answer: (ReplayStatus, Option<ReplayEvent>)) -> Result<Value, RpcError> {
    match answer.1 {
        Some(e) => body(e),
        None => Err(RpcError::internal("the event was changed and is not in the answer; call replay.event.list")),
    }
}

async fn get(call: Call, _params: Value) -> Result<Value, RpcError> {
    body(desk(&call, ReplayCall::Get).await?.0)
}

async fn start(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ReplayConfig = call.params(&params)?;
    body(desk(&call, ReplayCall::Start(req)).await?.0)
}

async fn stop(call: Call, _params: Value) -> Result<Value, RpcError> {
    body(desk(&call, ReplayCall::Stop).await?.0)
}

async fn mark(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: MarkRequest = call.params(&params)?;
    event_of(desk(&call, ReplayCall::Mark(req)).await?)
}

async fn list(call: Call, _params: Value) -> Result<Value, RpcError> {
    body(EventList { events: desk(&call, ReplayCall::Get).await?.0.events })
}

async fn rename(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: EventRenameRequest = call.params(&params)?;
    event_of(desk(&call, ReplayCall::Rename(req)).await?)
}

async fn delete(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: EventIdRequest = call.params(&params)?;
    desk(&call, ReplayCall::Delete(req.id.clone())).await?;
    body(req)
}
