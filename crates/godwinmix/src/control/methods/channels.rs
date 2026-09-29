//! RTMP channels: list, get, add, set, remove, and a channel's keys.
//!
//! The work is in `crate::channels`. Each handler runs it on the blocking
//! pool, because it writes a file, seals a key and tells the RTMP listener,
//! and the listener gets up to the protocol's five seconds to answer.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_protocol::channels::*;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{no_params, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::requests::IdRequest;
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "channel.list",
            Scope::Read,
            "Every RTMP channel with its keys (as hints), the address to publish to, and \
             what is live on it, beside the port they all share.",
            handler(|call: Call, _| async move { run(&call, |c| Ok(c.list())).await }),
        )
        .params(no_params)
        .result(schema_of::<ChannelList>)
        .tool(
            "list_channels",
            Tier::Search,
            "The RTMP channels encoders publish to on this mixer, and each live stream on \
             them with its codec, size, frame rate, bit rate and the mixer source it feeds. \
             Keys are never shown, only their last four characters.",
        ),
    );
    reg.register(
        MethodDef::new("channel.get", Scope::Read, "One channel.", handler(get))
            .params(schema_of::<IdRequest>)
            .result(schema_of::<Channel>),
    );
    reg.register(
        MethodDef::new(
            "channel.add",
            Scope::Admin,
            "Make a channel and its first key. The key is in this answer and never again.",
            handler(add),
        )
        .params(schema_of::<ChannelAddRequest>)
        .result(schema_of::<ChannelAdded>)
        .not_idempotent(),
    );
    reg.register(
        MethodDef::new(
            "channel.set",
            Scope::Admin,
            "Rename a channel, switch it on or off, or change its application name, \
             whether its streams become sources, or how its key is given. Only what is \
             named moves.",
            handler(set),
        )
        .params(schema_of::<ChannelSetRequest>)
        .result(schema_of::<Channel>),
    );
    reg.register(
        MethodDef::new(
            "channel.remove",
            Scope::Admin,
            "Remove a channel and forget its keys. Sources it made that no scene holds go \
             with it.",
            handler(remove),
        )
        .params(schema_of::<IdRequest>)
        .result(schema_of::<ChannelRemoved>)
        .destructive(),
    );
    reg.register(
        MethodDef::new(
            "channel.key.add",
            Scope::Admin,
            "Make another key for a channel, to give to one more person or encoder. The key \
             is in this answer and never again.",
            handler(key_add),
        )
        .params(schema_of::<ChannelKeyAddRequest>)
        .result(schema_of::<KeyAdded>)
        .not_idempotent(),
    );
    reg.register(
        MethodDef::new(
            "channel.key.remove",
            Scope::Admin,
            "Take one key back. The next publisher with it is turned away; the other keys \
             are untouched.",
            handler(key_remove),
        )
        .params(schema_of::<ChannelKeyRemoveRequest>)
        .result(schema_of::<Channel>)
        .destructive(),
    );
}

/// Run one piece of channel work on the blocking pool.
async fn run<T, F>(call: &Call, work: F) -> Result<Value, RpcError>
where
    T: serde::Serialize + Send + 'static,
    F: FnOnce(&crate::channels::Channels) -> Result<T, RpcError> + Send + 'static,
{
    let channels = call.app.channels.clone();
    let answer = tokio::task::spawn_blocking(move || work(&channels))
        .await
        .map_err(|e| RpcError::internal(format!("the channel work stopped: {e}")))??;
    body(answer)
}

async fn get(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: IdRequest = call.params(&params)?;
    run(&call, move |c| c.get(&req.id)).await
}

async fn add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ChannelAddRequest = call.params(&params)?;
    run(&call, move |c| c.add(req)).await
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ChannelSetRequest = call.params(&params)?;
    run(&call, move |c| c.set(req)).await
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: IdRequest = call.params(&params)?;
    run(&call, move |c| c.remove(&req.id)).await
}

async fn key_add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ChannelKeyAddRequest = call.params(&params)?;
    run(&call, move |c| c.key_add(req)).await
}

async fn key_remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ChannelKeyRemoveRequest = call.params(&params)?;
    run(&call, move |c| c.key_remove(req)).await
}
