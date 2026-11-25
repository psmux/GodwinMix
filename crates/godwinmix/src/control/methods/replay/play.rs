//! `replay.play` and `replay.set`: a replay source loaded, sped up or slowed
//! down, and its angle changed, through the same seek a scrubber makes.

use super::super::{body, handler};
use super::refusal;
use crate::control::call::Call;
use godwinmix_core::input::InputPipeline;
use godwinmix_core::mixer::SeekOutcome;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::replay::{PlayRequest, PlayStatus, ReplaySetRequest, DEFAULT_SOURCE};
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("replay.play", Scope::Operate, "Play an event, or the last seconds of an angle, on a replay source, added if it is not there.", handler(play))
            .params(schema_of::<PlayRequest>)
            .result(schema_of::<PlayStatus>)
            .tool(
                "play_replay",
                Tier::Search,
                "Play a replay: `event` (an id from mark_replay) or, without one, the last `seconds` (default 10) \
                 of `angle`. `speed` is 1.0 real time, 0.5 half, 0.25 quarter, down to 0.1; slower than 1.0 is \
                 silent. It plays on the source `replay`, added the first time, which goes on air like any \
                 source; `take: true` puts it on air now, with `transition` if given.",
            ),
    );
    reg.register(
        MethodDef::new("replay.set", Scope::Operate, "Change a playing replay's angle or speed, from the frame it is on.", handler(set))
            .params(schema_of::<ReplaySetRequest>)
            .result(schema_of::<PlayStatus>)
            .tool("set_replay", Tier::Search, "Change the angle or the speed of the replay that is playing. It carries on from the moment it was showing."),
    );
}

/// An error from a replay source's kind, as the protocol says it.
fn kind_error(e: anyhow::Error) -> RpcError {
    match e.downcast::<godwinmix_core::replay::Refusal>() {
        Ok(r) => refusal(r),
        Err(e) => RpcError::not_in_state(format!("{e:#}")),
    }
}

async fn kind_call(input: &Arc<InputPipeline>, method: &'static str, params: Value) -> Result<Value, RpcError> {
    let input = input.clone();
    tokio::task::spawn_blocking(move || input.call(method, params))
        .await
        .map_err(|e| RpcError::internal(format!("the replay source stopped answering: {e}")))?
        .map_err(kind_error)
}

/// The replay source called `id`, refusing a source of another kind.
async fn replay_input(call: &Call, id: &str) -> Result<Option<Arc<InputPipeline>>, RpcError> {
    let Some(input) = call.app.mixer.source_input(id).await.map_err(|e| call.mixer_error(e))? else { return Ok(None) };
    if input.manifest().plugin != "replay" {
        return Err(RpcError::not_in_state(format!(
            "source {id} is a {} source, not a replay. Name a replay source with `source`, or leave it out to use `{DEFAULT_SOURCE}`",
            input.type_id()
        ))
        .with("source", id));
    }
    Ok(Some(input))
}

async fn seek(call: &Call, id: &str, position_ms: u64) -> Result<(), RpcError> {
    match call.app.mixer.seek(id.into(), position_ms).await.map_err(|e| call.mixer_error(e))? {
        SeekOutcome::Moved(_) => Ok(()),
        SeekOutcome::Failed(why) => Err(RpcError::not_in_state(format!("the replay could not move: {why}"))),
        _ => Err(RpcError::not_in_state(format!("replay source {id} is still starting; try again in a moment")).with("source", id)),
    }
}

fn status(id: &str, state: &Value) -> PlayStatus {
    let s = state.get("state").unwrap_or(state);
    PlayStatus {
        source: id.to_string(),
        event: s["event"].as_str().map(str::to_string),
        angle: s["angle"].as_str().unwrap_or_default().to_string(),
        speed: s["speed"].as_f64().unwrap_or(1.0),
        duration_ms: s["duration_ms"].as_u64().unwrap_or(0),
        position_ms: state["position_ms"].as_u64().or(s["position_ms"].as_u64()).unwrap_or(0),
    }
}

async fn play(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PlayRequest = call.params(&params)?;
    let id = req.source.clone().unwrap_or_else(|| DEFAULT_SOURCE.to_string());
    godwinmix_core::replay::shared::check_speed(req.speed).map_err(refusal)?;
    // Refused here, before a source is added for a replay that holds nothing.
    let shared = call.app.mixer.replay_shared();
    let window = shared.window(req.event.as_deref(), req.seconds, req.angle.as_deref()).map_err(refusal)?;
    shared.clip(&window, &window.angle).map_err(refusal)?;
    let mut load = json!({ "angle": window.angle, "speed": req.speed.unwrap_or(1.0) });
    match (&req.event, req.seconds) {
        (Some(e), _) => load["event"] = json!(e),
        (None, Some(s)) => load["seconds"] = json!(s),
        _ => {}
    }
    let input = match replay_input(&call, &id).await? {
        Some(input) => {
            kind_call(&input, "load", load).await?;
            seek(&call, &id, 0).await?;
            input
        }
        None => {
            let add = json!({ "id": id, "uri": "replay://", "params": load });
            super::super::project::invoke(&call, "source.add", add).await?;
            replay_input(&call, &id).await?.ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("replay source {id} went as it was added")))?
        }
    };
    if req.take {
        let take = json!({ "source": id, "transition": req.transition });
        super::super::project::invoke(&call, "program.take", take).await?;
    }
    body(status(&id, &kind_call(&input, "state", json!({})).await?))
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ReplaySetRequest = call.params(&params)?;
    let id = req.source.clone().unwrap_or_else(|| DEFAULT_SOURCE.to_string());
    let input = replay_input(&call, &id).await?.ok_or_else(|| {
        RpcError::new(ErrorCode::NotFound, format!("there is no replay source {id}. Start one with replay.play")).with("next", "replay.play")
    })?;
    let answer = kind_call(&input, "set", json!({ "angle": req.angle, "speed": req.speed })).await?;
    seek(&call, &id, answer["position_ms"].as_u64().unwrap_or(0)).await?;
    body(status(&id, &answer))
}
