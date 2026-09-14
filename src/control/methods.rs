//! Every method the core implements, as rows in a table.
//!
//! This is the only place a method is declared. The `/rpc` dispatcher, the
//! `/api/v1` router, `protocol.json` and the MCP tool list are all built from
//! what is here, so a method cannot exist on one surface and not another.
//!
//! The descriptions on the MCP bindings are long on purpose: they are an
//! agent's only manual for the mixer, and the two profiles are budgeted in
//! bytes by a test in mcp.rs, which fails rather than letting them grow.

use crate::api::error::{ErrorCode, RpcError};
use crate::api::method::{any_object, schema_of, Handler, MethodDef, Registry, Tier};
use crate::api::requests::*;
use crate::api::scope::Scope;
use crate::api::types::*;
use crate::control::call::Call;
use serde_json::{json, Value};
use std::future::Future;
use std::sync::Arc;

mod media;
mod outputs;
mod program;
mod sources;

/// Wrap an async function as a handler, so a registration reads as one thing.
pub(crate) fn handler<F, Fut>(f: F) -> Handler<Call>
where
    F: Fn(Call, Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, RpcError>> + Send + 'static,
{
    Arc::new(move |call, params| Box::pin(f(call, params)))
}

/// Turn a serialisable answer into the body a method returns.
pub(crate) fn body<T: serde::Serialize>(value: T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::internal(format!("encoding the answer: {e}")))
}

/// The whole table.
///
/// A pure function: it builds every handler without touching a mixer, which
/// is what lets `godwinmix --api-info` print the protocol on a machine with
/// no GStreamer and no configuration.
pub fn registry() -> Registry<Call> {
    let mut reg = Registry::new();
    register_core(&mut reg);
    register_introspection(&mut reg);
    program::register(&mut reg);
    sources::register(&mut reg);
    outputs::register(&mut reg);
    media::register(&mut reg);
    // Other modules add their own here. One line each, and they land on
    // /rpc, /api/v1, protocol.json and the tool list together. See
    // src/api/README.md.
    reg
}

fn register_core(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "core.info",
            Scope::Read,
            "What this core is, what it can do, and where its edges are.",
            handler(|call: Call, _| async move {
                body(CoreInfo {
                    core: "godwinmix".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    api_level: crate::api::API_LEVEL,
                    api_compatible: crate::api::API_COMPATIBLE,
                    features: call.app.features.as_ref().clone(),
                    limits: call.app.limits.clone(),
                    canvas: call.app.canvas,
                    token: Some(call.token.info()),
                    rehearsal: call.app.rehearsal,
                })
            }),
        )
        .result(schema_of::<CoreInfo>)
        .tool(
            "core_info",
            Tier::Search,
            "Version, api_level, canvas size, the features this build has (multiview, \
             snapshot, uploads, browser) and the limits. Read it once at the start if you \
             need to know whether snapshots or uploads exist here before trying them.",
        ),
    );

    reg.register(
        MethodDef::new(
            "core.api",
            Scope::Read,
            "Every method, event and type as JSON Schema. The same document as \
             protocol.json and `godwinmix --api-info`.",
            handler(|_call: Call, _| async move { Ok(crate::control::descriptor().clone()) }),
        )
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "core.status",
            Scope::Read,
            "The full state: programme, every source, every output, the multiview grid, \
             the encoder backend and any ad break.",
            handler(|call: Call, _| async move {
                let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
                body(status)
            }),
        )
        .result(schema_of::<MixerStatus>)
        .tool(
            "status",
            Tier::Standard,
            "Full snapshot of the mixer: what is on programme, every source with its id, \
             name, URL, connection state and whether it has video and audio right now, \
             every output with its state and reconnect count, the encoder backend, and any \
             ad break. Use it after a change to confirm it happened. Prefer `agent_state` \
             when you are deciding what to put on air, because it is smaller and carries \
             motion scores.",
        ),
    );

    reg.register(
        MethodDef::new(
            "core.subscribe",
            Scope::Read,
            "Subscribe to the event stream. WebSocket only: the core answers \
             event/snapshot then deltas, ending every batch with event/flush.",
            handler(|call: Call, _| async move {
                Err(RpcError::new(
                    ErrorCode::NotInState,
                    format!(
                        "core.subscribe needs a connection that stays open, and {} is not one. \
                         Open a WebSocket to /rpc and send it there.",
                        call.method
                    ),
                ))
            }),
        )
        .params(schema_of::<SubscribeRequest>)
        .result(schema_of::<SubscribeResult>)
        .mutating(false)
        .no_rest(),
    );
}

/// What the core will tell you about itself and the machine it is on.
fn register_introspection(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "agent.state",
            Scope::Read,
            "The compact document written for agents: the programme, each source's state \
             and a motion score saying how much its picture is changing.",
            handler(|call: Call, _| async move {
                let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
                body(crate::snapshot::agent_state(&status, call.snapshots.latest().as_ref()))
            }),
        )
        .result(any_object)
        .tool(
            "agent_state",
            Tier::Minimal,
            "Compact state written for agents, a few hundred tokens: the programme source, \
             each source's id and state, and a motion score saying how much its picture is \
             changing, so you can tell a live camera from a frozen or black one without \
             looking at it. Start here. Use `snapshot` only when you need to see what is \
             actually in the shot.",
        ),
    );

    reg.register(
        MethodDef::new(
            "codec.list",
            Scope::Read,
            "The codecs and elements this machine can use. The catalogue proper is not \
             built yet; this reports the backends actually selected.",
            handler(|call: Call, _| async move {
                // TODO(codecs agent): answer from `crate::catalogue::list()`
                // when it exists. The route, the scope and the shape are
                // settled here so that filling it in is one function body and
                // no client has to change.
                let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
                Ok(json!({
                    "catalogue": Value::Null,
                    "note": "the codec catalogue is not built in this release; these are the \
                             backends this core selected at startup",
                    "selected": status.backend,
                }))
            }),
        )
        .result(any_object)
        .tool(
            "list_codecs",
            Tier::Search,
            "Which video and audio encoder and decoder this machine picked, and whether \
             they are hardware accelerated. Use it when a source or an output is slow and \
             you want to know whether the box is encoding in software.",
        ),
    );

    reg.register(
        MethodDef::new(
            "core.shutdown",
            Scope::Admin,
            "Stop the mixer, and with it the programme. Nothing else takes the show off \
             air, so this is deliberately its own call.",
            handler(|call: Call, _| async move {
                if call.dry_run {
                    return Ok(call.dry_run_answer(
                        true,
                        vec!["stop the programme and exit the process".into()],
                    ));
                }
                tracing::info!(trace_id = %call.trace_id, token = %call.token.id, "shutdown requested");
                call.app.quit.notify_one();
                Ok(json!({ "stopping": true }))
            }),
        )
        .destructive(),
    );
}
