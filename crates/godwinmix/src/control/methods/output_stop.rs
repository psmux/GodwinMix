//! `output.stop` and `output.start`: stop sending to a destination and keep
//! it, then send to it again.
//!
//! `output.remove` was the only way to stop a stream, and it forgot the
//! stream key with the destination. These two keep the destination, its
//! address and its key, so stopping is something a person can do without
//! losing anything, and the next Start is one press. See `mixer::held`.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::mixer::Command;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::requests::IdRequest;
use godwinmix_protocol::types::OutputStatus;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "output.stop",
            godwinmix_protocol::scope::Scope::Operate,
            "Stop sending the programme to one destination and keep it, address, key \
             and all, for output.start. Viewers on that platform see the stream end.",
            handler(stop),
        )
        .params(schema_of::<IdRequest>)
        .result(schema_of::<OutputStatus>)
        // Not destructive: output.start undoes it, with the same key, the
        // way switching a channel destination off and on again does.
        .tool(
            "stop_output",
            Tier::Search,
            "Stop sending the programme to one destination, for instance YouTube, and keep \
             the destination with its stream key so start_output sends to it again. The \
             viewers on that platform see the stream end; the programme and every other \
             output carry on. Stopping one already stopped changes nothing. Returns the \
             output, now `stopped`.",
        ),
    );

    reg.register(
        MethodDef::new(
            "output.start",
            godwinmix_protocol::scope::Scope::Operate,
            "Send to a stopped destination again, with the address and key it kept.",
            handler(start),
        )
        .params(schema_of::<IdRequest>)
        .result(schema_of::<OutputStatus>)
        .tool(
            "start_output",
            Tier::Search,
            "Start sending the programme again to a destination that output.stop stopped, \
             with the address and stream key it kept. Starting one that is already sending \
             changes nothing. Returns the output, which then connects as a new one does.",
        ),
    );
}

async fn stop(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: IdRequest = call.params(&params)?;
    find(&call, &req.id).await?;
    call.app
        .mixer
        .request(|ack| Command::StopOutput(req.id.clone(), Some(ack)))
        .await
        .map_err(|e| call.mixer_error(e))?;
    body(find(&call, &req.id).await?)
}

async fn start(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: IdRequest = call.params(&params)?;
    find(&call, &req.id).await?;
    call.app
        .mixer
        .request(|ack| Command::StartOutput(req.id.clone(), Some(ack)))
        .await
        .map_err(|e| call.mixer_error(e))?;
    body(find(&call, &req.id).await?)
}

/// The output by id, or an error that lists the ids there are.
async fn find(call: &Call, id: &str) -> Result<OutputStatus, RpcError> {
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    status.outputs.iter().find(|o| o.id == id).cloned().ok_or_else(|| {
        let ids = status.outputs.iter().map(|o| o.id.clone()).collect::<Vec<_>>();
        RpcError::not_found("output", id, &ids)
    })
}
