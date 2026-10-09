//! Channels: list, get, add, set, remove, a channel's keys, and the
//! certificate RTMPS answers with.
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
            "Every channel with its keys (as hints), the address to publish to over each \
             protocol it has on, and what is live on it; and which ingest ports are open \
             and for which channels.",
            handler(|call: Call, _| async move { run(&call, |c| Ok(c.list())).await }),
        )
        .params(no_params)
        .result(schema_of::<ChannelList>)
        .tool(
            "list_channels",
            Tier::Search,
            "The channels encoders publish to on this mixer (by RTMP, RTMPS, SRT or WHIP), \
             each live stream on them with its protocol, codec, size, frame rate, bit rate \
             and the mixer source it feeds, and which ports are open and why. Keys are \
             never shown, only their last four characters.",
        ),
    );
    reg.register(
        MethodDef::new("channel.get", Scope::Read, "One channel.", handler(get))
            .params(schema_of::<IdRequest>)
            .result(schema_of::<Channel>),
    );
    reg.register(
        MethodDef::new(
            "channel.thumbnail",
            Scope::Read,
            "A live channel stream's picture as a small JPEG in base64, {channel, stream, jpeg, \
             width, height, at_ms}, or {pending: true, retry_after_ms} while the first keyframe \
             is on its way. Keyframes only, about one a second, for ten seconds after an ask; \
             nothing is decoded between asks. GET /api/v1/channels/{id}/streams/{stream}/thumbnail.jpg \
             serves the JPEG itself.",
            handler(|call: Call, params| async move {
                let req: ChannelThumbnailRequest = call.params(&params)?;
                run(&call, move |c| c.thumbnail(&req)).await
            }),
        )
        .params(schema_of::<ChannelThumbnailRequest>)
        .result(godwinmix_protocol::method::any_object)
        .mutating(false),
    );
    reg.register(
        MethodDef::new(
            "channel.add",
            Scope::Admin,
            "Make a channel and its first key, which is in this answer. channel.key.reveal \
             reads it again later.",
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
             whether its streams become sources, how its key is given, which protocols it \
             takes (rtmp, srt, whip) or RTMPS and its port. A port opens when the first \
             channel needs it and closes when the last one stops. Only what is named moves.",
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
             is in this answer, and channel.key.reveal reads it again later.",
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
            "Take one key back. A publisher on air with it is cut off and the next one is \
             turned away; the other keys are untouched.",
            handler(key_remove),
        )
        .params(schema_of::<ChannelKeyRemoveRequest>)
        .result(schema_of::<Channel>)
        .destructive(),
    );
    reg.register(
        MethodDef::new(
            "channel.key.reveal",
            Scope::Admin,
            "Read one key of a channel back, to give it to an encoder again. Admin only; \
             a list shows only the last four characters. Each read is logged with who \
             asked, never with the key.",
            handler(key_reveal),
        )
        .params(schema_of::<ChannelKeyRevealRequest>)
        .result(schema_of::<KeyRevealed>)
        .mutating(false),
    );
}

/// The certificate RTMPS answers with, for every channel that turns it on.
pub fn register_certificate(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "channel.certificate.set",
            Scope::Admin,
            "Give RTMPS a certificate: the PEM of the certificate (and its chain) and of its \
             private key, as a certificate authority issued them. Checked before it is kept; \
             the key is sealed and never read back.",
            handler(|call: Call, params| async move {
                let req: CertificateSetRequest = call.params(&params)?;
                run(&call, move |c| c.certificate_set(req)).await
            }),
        )
        .params(schema_of::<CertificateSetRequest>)
        .result(schema_of::<CertificateInfo>)
        .not_idempotent(),
    );
    reg.register(
        MethodDef::new(
            "channel.certificate.generate",
            Scope::Admin,
            "Make a self signed certificate for RTMPS, for this machine's addresses unless \
             names are given. Encoders must be told to accept it; one from a certificate \
             authority needs no such step.",
            handler(|call: Call, params| async move {
                let req: CertificateGenerateRequest = call.params(&params)?;
                run(&call, move |c| c.certificate_generate(req)).await
            }),
        )
        .params(schema_of::<CertificateGenerateRequest>)
        .result(schema_of::<CertificateInfo>)
        .not_idempotent(),
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

async fn key_reveal(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ChannelKeyRevealRequest = call.params(&params)?;
    let caller = call.token.id.clone();
    run(&call, move |c| c.key_reveal(req, &caller)).await
}
