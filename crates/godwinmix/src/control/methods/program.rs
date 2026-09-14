//! What is on air: take, revert, history, and the one call go live.

use super::{body, handler};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::requests::*;
use godwinmix_protocol::scope::Scope;
use crate::control::call::Call;
use godwinmix_core::mixer::Command;
use serde_json::Value;

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
            "Put a source on programme. The cut is instant and the outgoing stream is not \
             disturbed.",
            handler(take),
        )
        .params(schema_of::<TakeRequest>)
        .result(schema_of::<ProgramState>)
        .tool(
            "take",
            Tier::Minimal,
            "Put a source on programme. The cut is instant and the outgoing stream is not \
             disturbed. Pass the source id from `agent_state`; omit it or pass null to cut \
             to the slate. `at_running_time_ms` schedules the cut on a frame instead of \
             now. Returns the new programme state, so no follow up read is needed. An \
             unknown id is refused with the ids that would have worked.",
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
        running_time_ms: status.running_time_ms,
        ad: status.ad,
    })
}

async fn take(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: TakeRequest = call.params(&params)?;
    let target = req.target();
    if let Some(id) = &target {
        let ids = call.source_ids().await;
        if !ids.contains(id) {
            return Err(RpcError::not_found("source", id, &ids));
        }
    }
    cut(&call, target, req.at_running_time_ms).await
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
    body(state(call).await?)
}

async fn golive(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GoLiveRequest = call.params(&params)?;
    let result = crate::control::golive_now(&call.app, req)
        .await
        .map_err(|e| call.mixer_error(e))?;
    body(result)
}
