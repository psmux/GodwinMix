//! Destinations: list, get, add, set, remove, reconnect.

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
            "output.set",
            Scope::Operate,
            "Change a destination in place: a new address with a new stream key, a new \
             reconnect policy, a deeper outage buffer. The address is write only, so a \
             client that only wants the buffer never has to hold the key.",
            handler(set),
        )
        .params(schema_of::<SetOutputRequest>)
        .result(schema_of::<OutputStatus>)
        .tool(
            "set_output",
            // Search, not Standard, beside `reconnect_output` and
            // `remove_output`. Correcting a destination is rare and always
            // deliberate, and the standard profile is a budget somebody else
            // has to live inside.
            Tier::Search,
            "Change one destination in place without losing its id: give it a new address \
             (the whole URL, stream key and all), a new reconnect `policy`, or a deeper \
             `queue_secs` outage buffer. Only the fields you name move. This is how a \
             placeholder stream key gets replaced: `list_outputs` shows `has_key` false \
             while the address a preset wrote still says YOUR-STREAM-KEY, and this method \
             puts the real one in. The destination is rebuilt, so it reconnects; the \
             programme and every other output are not disturbed. The address is never \
             read back by any method. Refused on a core started with --rehearsal.",
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

/// Change one destination, naming only what moves.
///
/// The merge is done here, against the config the mixer is actually running,
/// rather than by making the caller resend the whole record: an address it
/// cannot read back is not one it can echo, so a partial request is the only
/// honest shape this can have.
async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SetOutputRequest = call.params(&params)?;
    let configs = call.app.mixer.configs().await.map_err(|e| call.mixer_error(e))?;
    let Some(current) = configs.outputs.iter().find(|o| o.id == req.id).cloned() else {
        let ids = configs.outputs.iter().map(|o| o.id.clone()).collect::<Vec<_>>();
        return Err(RpcError::not_found("output", &req.id, &ids));
    };
    let wanted = merge(&req, current)?;

    if req.is_empty() {
        // Nothing was named, so there is nothing to rebuild a live
        // destination for. Answering with the record says so.
        return body(find(&call, &req.id).await?);
    }
    // One command. The mixer takes the output down and puts it back under the
    // same id on its own thread, so no status a client reads is ever missing
    // it, and the programme carries on: the encoder is shared and the other
    // destinations keep their connections.
    call.app
        .mixer
        .request(|ack| Command::SetOutput(Box::new(wanted), Some(ack)))
        .await
        .map_err(|e| call.mixer_error(e))?;
    body(find(&call, &req.id).await?)
}

/// The request laid over the config the output is running, and nothing else.
fn merge(
    req: &SetOutputRequest,
    current: godwinmix_core::config::OutputConfig,
) -> Result<godwinmix_core::config::OutputConfig, RpcError> {
    let mut wanted = current;
    if let Some(uri) = &req.uri {
        // Trimmed, because a stream key pasted out of a platform's dashboard
        // arrives with a newline on the end often enough to be worth handling
        // here as well as in the surface that took it.
        let uri = uri.trim();
        if uri.is_empty() {
            return Err(RpcError::invalid_params(format!(
                "output '{}' still needs an address. Send `uri` with the whole URL \
                 including the stream key, or leave `uri` out to keep the one it has.",
                req.id
            ))
            .with("id", req.id.clone())
            .with("field", "uri"));
        }
        if scheme_of(uri) != scheme_of(&wanted.uri) {
            // The kind is worked out from the address when `type` is absent,
            // so a move from rtmp:// to srt:// has to let it be worked out
            // again rather than keep the old kind's id.
            wanted.type_id = None;
        }
        // An output kind reads `params.uri` ahead of the config's own, so a
        // params copy left over from an earlier add would quietly win.
        wanted.params.remove("uri");
        wanted.extra.remove("uri");
        wanted.uri = uri.to_string();
    }
    if let Some(policy) = &req.policy {
        wanted.policy = match policy.as_str() {
            "own" => godwinmix_core::config::OutputPolicy::Own,
            "cdn" => godwinmix_core::config::OutputPolicy::Cdn,
            other => {
                return Err(RpcError::invalid_params(format!(
                    "'{other}' is not a reconnect policy. Send \"own\" for a server you \
                     run or \"cdn\" for a platform that penalises hammering."
                ))
                .with("id", req.id.clone())
                .with("field", "policy")
                .with("policies", vec!["own", "cdn"]))
            }
        };
    }
    if let Some(secs) = req.queue_secs {
        if !(0.0..=60.0).contains(&secs) || !secs.is_finite() {
            return Err(RpcError::invalid_params(format!(
                "an outage buffer of {secs} seconds is not usable. Send `queue_secs` \
                 between 0 and 60."
            ))
            .with("id", req.id.clone())
            .with("field", "queue_secs"));
        }
        wanted.queue_secs = secs;
    }
    for (key, value) in &req.params {
        match toml::Value::try_from(value) {
            Ok(v) => {
                wanted.params.insert(key.clone(), v);
            }
            Err(e) => {
                return Err(RpcError::invalid_params(format!(
                    "`{key}` is not something an output config can hold: {e}. Send a \
                     string, a number or a boolean."
                ))
                .with("id", req.id.clone())
                .with("field", key.clone()))
            }
        }
    }
    Ok(wanted)
}

/// The scheme, lowercased, or "" for an address that has none.
fn scheme_of(uri: &str) -> String {
    uri.split_once("://").map(|(s, _)| s.to_lowercase()).unwrap_or_default()
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
