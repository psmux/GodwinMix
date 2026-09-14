//! Destinations: list, get, add, remove, reconnect.

use super::{body, handler};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::requests::*;
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::*;
use crate::control::call::Call;
use godwinmix_core::mixer::Command;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "output.list",
            Scope::Read,
            "Every destination, with its state, reconnect count and how much is buffered.",
            handler(|call: Call, _| async move {
                let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
                body(status.outputs)
            }),
        )
        .result(schema_of::<Vec<OutputStatus>>)
        .tool(
            "list_outputs",
            Tier::Standard,
            "The RTMP destinations the programme is being sent to, with each one's id, \
             host, connection state, reconnect count and how many seconds are buffered. A \
             queue that climbs and stays high means the destination cannot keep up. Use it \
             to check the broadcast is actually arriving somewhere.",
        ),
    );

    reg.register(
        MethodDef::new(
            "output.get",
            Scope::Read,
            "One destination.",
            handler(|call: Call, params| async move {
                let req: IdRequest = call.params(&params)?;
                body(find(&call, &req.id).await?)
            }),
        )
        .params(schema_of::<IdRequest>)
        .result(schema_of::<OutputStatus>),
    );

    reg.register(
        MethodDef::new(
            "output.add",
            Scope::Operate,
            "Send the programme to another destination. The encoder is shared, so adding \
             one costs nothing on air.",
            handler(add),
        )
        .params(schema_of::<AddOutputRequest>)
        .result(schema_of::<OutputStatus>)
        .tool(
            "add_output",
            Tier::Standard,
            "Start sending the programme to another RTMP destination, for instance a \
             YouTube or Twitch ingest URL with the stream key on the end. The output \
             encoder is shared, so adding one costs nothing on air. `policy` sets the \
             reconnect behaviour: \"own\" retries quickly, for a server you run; \"cdn\" \
             backs off harder, for a platform that penalises hammering. Refused outright on \
             a core started with --rehearsal, so a rehearsal cannot reach a real \
             destination. Returns the output record.",
        ),
    );

    reg.register(
        MethodDef::new(
            "output.remove",
            Scope::Operate,
            "Stop sending to a destination and forget it. Other outputs are unaffected.",
            handler(remove),
        )
        .params(schema_of::<IdRequest>)
        .destructive()
        .tool(
            "remove_output",
            Tier::Search,
            "Stop sending the programme to one destination and forget it. Other outputs \
             keep running and the programme is not disturbed. Pass dry_run true to see \
             what it would do first.",
        ),
    );

    reg.register(
        MethodDef::new(
            "output.reconnect",
            Scope::Operate,
            "Drop and re-establish one destination's connection now, without waiting for \
             its reconnect policy.",
            handler(|call: Call, params| async move {
                let req: IdRequest = call.params(&params)?;
                find(&call, &req.id).await?;
                call.app
                    .mixer
                    .request(|ack| Command::ReconnectOutput(req.id.clone(), Some(ack)))
                    .await
                    .map_err(|e| call.mixer_error(e))?;
                body(find(&call, &req.id).await?)
            }),
        )
        .params(schema_of::<IdRequest>)
        .result(schema_of::<OutputStatus>)
        .tool(
            "reconnect_output",
            Tier::Search,
            "Force one destination to drop and re-establish its RTMP connection now, \
             without waiting for its reconnect policy. Use it when `list_outputs` shows an \
             output stuck, or the platform reports no data arriving while the mixer thinks \
             it is connected.",
        ),
    );
}

async fn find(call: &Call, id: &str) -> Result<OutputStatus, RpcError> {
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    status.outputs.iter().find(|o| o.id == id).cloned().ok_or_else(|| {
        let ids = status.outputs.iter().map(|o| o.id.clone()).collect::<Vec<_>>();
        RpcError::not_found("output", id, &ids)
    })
}

async fn add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: AddOutputRequest = call.params(&params)?;
    let cfg: godwinmix_core::config::OutputConfig = serde_json::from_value(req.to_config_json())
        .map_err(|e| RpcError::invalid_params(format!("that is not a usable output: {e}")))?;
    let id = cfg.id.clone();
    call.app
        .mixer
        .request(|ack| Command::AddOutput(Box::new(cfg), Some(ack)))
        .await
        .map_err(|e| call.mixer_error(e))?;
    body(find(&call, &id).await?)
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: IdRequest = call.params(&params)?;
    let output = find(&call, &req.id).await?;
    if call.dry_run {
        return Ok(call.dry_run_answer(
            true,
            vec![format!("stop sending the programme to {} ({})", output.id, output.uri_host)],
        ));
    }
    call.app
        .mixer
        .request(|ack| Command::RemoveOutput(req.id.clone(), Some(ack)))
        .await
        .map_err(|e| call.mixer_error(e))?;
    let after = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    Ok(serde_json::json!({
        "removed": req.id,
        "outputs": after.outputs.iter().map(|o| o.id.clone()).collect::<Vec<_>>(),
    }))
}
