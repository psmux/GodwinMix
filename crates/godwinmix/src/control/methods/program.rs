//! What is on air: take, revert, history, and the one call go live.

use super::{body, handler};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::requests::*;
use godwinmix_protocol::scope::Scope;
use crate::control::call::Call;
use godwinmix_core::mixer::Command;
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "program.get",
            Scope::Read,
            "What is on air, the programme running time, and what revert would go back to.",
            handler(|call: Call, _| async move { body(state(&call).await?) }),
        )
        .result(schema_of::<ProgramState>),
    );

    reg.register(
        MethodDef::new(
            "program.take",
            Scope::Operate,
            "Put a scene or a source on programme. The cut is instant and the outgoing \
             stream is not disturbed.",
            handler(take),
        )
        .params(schema_of::<TakeRequest>)
        .result(schema_of::<ProgramState>)
        .tool(
            "take",
            Tier::Minimal,
            "Put a scene or a source on programme. The cut is instant and the stream is \
             not disturbed. Pass `source` with an id from `agent_state`, or `scene` with \
             a name, or neither to take the armed scene. Returns the programme state.",
        ),
    );

    reg.register(
        MethodDef::new(
            "program.revert",
            Scope::Operate,
            "Take back to the shot before this one.",
            handler(revert),
        )
        .result(schema_of::<ProgramState>)
        .tool(
            "revert",
            Tier::Standard,
            "Undo the last cut: take back to the shot that was on air before this one. Use \
             it the moment a take turns out to be wrong, rather than working out by hand \
             what was on before. Refused, with an explanation, when nothing has been taken \
             yet. Returns the new programme state.",
        ),
    );

    reg.register(
        MethodDef::new(
            "program.history",
            Scope::Read,
            "The last hundred takes, newest first, with the token that asked for each.",
            handler(|call: Call, params| async move {
                let req: HistoryRequest = call.params(&params)?;
                body(call.app.history.recent(req.limit.unwrap_or(20) as usize))
            }),
        )
        .params(schema_of::<HistoryRequest>)
        .result(schema_of::<Vec<TakeRecord>>)
        .tool(
            "program_history",
            Tier::Search,
            "What has been on air and who put it there: the last takes, newest first, each \
             with the source, the programme running time it landed on and the token id \
             that asked. Use it to work out what happened during a show.",
        ),
    );

    reg.register(
        MethodDef::new(
            "program.golive",
            Scope::Operate,
            "One call to put a web page on air: add the page, add the destination, and \
             take the page as soon as it renders.",
            handler(golive),
        )
        .params(schema_of::<GoLiveRequest>)
        .result(schema_of::<GoLiveResult>)
        .tool(
            "go_live",
            Tier::Standard,
            "One call to put a web page on air: add the page as a web source, add the RTMP \
             destination, and take the page to programme as soon as it renders. Use it when \
             someone says \"stream this page to that RTMP URL\" and nothing is set up yet. \
             The page can take a while to load, so this answers at once with the source id \
             and its state; the take happens by itself when the page produces a frame. \
             Reuses a source and an output that already point at the same addresses rather \
             than starting a second browser.",
        ),
    );
}

/// The programme, as every method that changes it answers with.
async fn state(call: &Call) -> Result<ProgramState, RpcError> {
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(anyhow::anyhow!(e)))?;
    Ok(ProgramState {
        previous: call.app.history.previous(status.program.as_deref()).flatten(),
        program: status.program,
        scene: status.scene,
        preview: call
            .app
            .scenes
            .armed()
            .and_then(|id| call.app.scenes.scene(&id.to_string()).ok())
            .map(|s| s.name),
        running_time_ms: status.running_time_ms,
        ad: status.ad,
    })
}

/// The take, widened for scenes.
///
/// Four shapes, in the order they are decided: a source id is shorthand for a
/// one item full canvas scene and is taken as it always was; a scene name is
/// taken as a scene; neither, with a scene armed, takes the armed one; neither
/// with nothing armed cuts to the slate. A name that is both a source id and a
/// scene name is read as the source, because `source` is the older word and
/// everything built on it has to keep working.
async fn take(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: TakeRequest = call.params(&params)?;
    req.check_transition().map_err(|e| {
        RpcError::invalid_params(e).with("transitions", json!(godwinmix_protocol::requests::TRANSITIONS))
    })?;

    // A name that is wrong is answered before a safety rule is consulted: a
    // caller who typed the wrong id needs to hear that, not how long the hold
    // has left.
    if let Some(id) = req.source_id() {
        let ids = call.source_ids().await;
        if !ids.contains(&id) {
            return Err(RpcError::not_found("source", &id, &ids));
        }
        call.app.safety.check(&call.token).map_err(|r| call.safety_error(r))?;
        return cut(&call, Some(id), req.at_running_time_ms).await;
    }

    // A scene by name, or the armed one when nothing was named.
    let named = match req.scene_name() {
        Some(name) => Some(name),
        None => call.app.scenes.armed().map(|id| id.to_string()),
    };
    let Some(which) = named else {
        // Nothing named and nothing armed: the slate, which is what
        // `program.take {}` has always meant.
        call.app.safety.check(&call.token).map_err(|r| call.safety_error(r))?;
        return cut(&call, None, req.at_running_time_ms).await;
    };
    take_scene(&call, &which, req.at_running_time_ms).await
}

/// Put a whole scene on air.
async fn take_scene(
    call: &Call,
    which: &str,
    at_running_time_ms: Option<u64>,
) -> Result<Value, RpcError> {
    // A draft of this scene taken off air is written back now, which is what
    // "applied on the next take" means.
    if let Ok(view) = call.app.scenes.scene(which) {
        call.app.scenes.apply_drafts_of(Some(&call.token.id), view.id);
    }
    let (name, placements) = call
        .app
        .scenes
        .placements(which)
        .map_err(|e| super::scenes::scene_error(call, e))?;
    // Every source the scene draws has to be here, or it is a composition with
    // holes in it and the caller should know before it is on air.
    let ids = call.source_ids().await;
    let missing: Vec<String> = {
        let mut m: Vec<String> =
            placements.iter().map(|p| p.source.clone()).filter(|s| !ids.contains(s)).collect();
        m.sort();
        m.dedup();
        m
    };
    if !missing.is_empty() {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!(
                "the scene {name:?} draws {} this mixer does not have. Add {} with \
                 source.add, or take a scene whose sources are all here. Sources here: {}",
                if missing.len() == 1 { "a source" } else { "sources" },
                missing.join(", "),
                if ids.is_empty() { "none".into() } else { ids.join(", ") }
            ),
        )
        .with("missing", json!(missing))
        .with("scene", name));
    }
    // The same rules a source take goes through, and in the same place: after
    // the names have been checked and before the pipeline is touched.
    call.app.safety.check(&call.token).map_err(|r| call.safety_error(r))?;
    call.app.history.expect(&call.token.id);
    call.app
        .mixer
        .request(|ack| Command::TakeScene {
            scene: Box::new(godwinmix_core::mixer::ProgramScene { name: name.clone(), placements }),
            at_running_time_ms,
            // A take is a cut. A duration belongs to a geometry command on a
            // scene that is already on air, not to putting one there.
            duration_ms: None,
            ack: Some(ack),
        })
        .await
        .map_err(|e| call.mixer_error(e))?;
    // Only once the mixer has taken it, exactly as `cut` does: a take the
    // pipeline refused must not start the hold on the next one.
    call.app.safety.record(&call.token.id);
    body(state(call).await?)
}

async fn revert(call: Call, _params: Value) -> Result<Value, RpcError> {
    let now = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    let Some(previous) = call.app.history.previous(now.program.as_deref()) else {
        return Err(RpcError::new(
            ErrorCode::NotInState,
            format!(
                "there is no earlier shot to revert to: {} takes have been recorded on this \
                 core. Use program.take with a source id instead.",
                call.app.history.len()
            ),
        )
        .with("program", now.program.clone()));
    };
    // Revert is held to the rate limit and to the flash guard, but not to the
    // minimum hold. The whole point of it is to undo a take that turned out
    // wrong, and a revert that has to wait eight seconds is not one.
    call.app.safety.check_revert(&call.token).map_err(|r| call.safety_error(r))?;
    cut(&call, previous, None).await
}

/// The one place a take is asked for, so that the history is claimed and the
/// answer is built the same way however the cut was decided.
async fn cut(
    call: &Call,
    source: Option<String>,
    at_running_time_ms: Option<u64>,
) -> Result<Value, RpcError> {
    call.app.history.expect(&call.token.id);
    call.app
        .mixer
        .request(|ack| Command::Take { source, at_running_time_ms, ack: Some(ack) })
        .await
        .map_err(|e| call.mixer_error(e))?;
    // Only once the mixer has taken it: a cut the pipeline refused must not
    // start the hold on the next one.
    call.app.safety.record(&call.token.id);
    body(state(call).await?)
}

async fn golive(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GoLiveRequest = call.params(&params)?;
    let result = crate::control::golive_now(&call.app, req)
        .await
        .map_err(|e| call.mixer_error(e))?;
    body(result)
}
