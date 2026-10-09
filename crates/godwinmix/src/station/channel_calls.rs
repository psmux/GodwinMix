//! `channel.*` under a station: the same work the single process core does
//! in `control/methods/channels.rs`, on the station's one registry of
//! channels, which the ingest plugin in the station serves.

use super::state::Station;
use crate::channels::Channels;
use crate::control::methods::channel_destinations as dest;
use godwinmix_protocol::channels::*;
use godwinmix_protocol::destination::{AddDestinationRequest, RemoveDestinationRequest, SetDestinationRequest};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::requests::IdRequest;
use godwinmix_protocol::scope::Token;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

fn parse<T: DeserializeOwned>(method: &str, params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|e| RpcError::invalid_params(format!("{method}: {e}")))
}

fn out<T: Serialize>(v: Result<T, RpcError>) -> Result<Value, RpcError> {
    v.map(|v| serde_json::to_value(v).unwrap_or_default())
}

pub async fn call(st: &Arc<Station>, token: &Token, method: &str, params: Value) -> Result<Value, RpcError> {
    let channels = st.channels.get().cloned().ok_or_else(|| RpcError::not_in_state("the channels are not open yet; try again in a moment"))?;
    let method = method.to_string();
    let caller = token.id.clone();
    tokio::task::spawn_blocking(move || run(&channels, &method, params, &caller))
        .await
        .map_err(|e| RpcError::internal(format!("the channel work stopped: {e}")))?
}

fn run(c: &Channels, method: &str, params: Value, caller: &str) -> Result<Value, RpcError> {
    match method {
        "channel.list" => out(Ok(c.list())),
        "channel.get" => out(c.get(&parse::<IdRequest>(method, params)?.id)),
        "channel.thumbnail" => c.thumbnail(&parse::<ChannelThumbnailRequest>(method, params)?),
        "channel.add" => out(c.add(parse::<ChannelAddRequest>(method, params)?)),
        "channel.set" => out(c.set(parse::<ChannelSetRequest>(method, params)?)),
        "channel.remove" => out(c.remove(&parse::<IdRequest>(method, params)?.id)),
        "channel.key.add" => out(c.key_add(parse(method, params)?)),
        "channel.key.remove" => out(c.key_remove(parse(method, params)?)),
        "channel.key.reveal" => out(c.key_reveal(parse(method, params)?, caller)),
        "channel.certificate.set" => out(c.certificate_set(parse::<CertificateSetRequest>(method, params)?)),
        "channel.certificate.generate" => out(c.certificate_generate(parse::<CertificateGenerateRequest>(method, params)?)),
        "channel.destination.add" => dest::add(c, &parse::<AddDestinationRequest>(method, params)?),
        "channel.destination.set" => dest::set(c, &parse::<SetDestinationRequest>(method, params)?),
        "channel.destination.remove" => {
            if params.get("dry_run").and_then(Value::as_bool) == Some(true) {
                return Err(RpcError::invalid_params("a station does not describe a destination's removal ahead of time; call it without dry_run"));
            }
            dest::remove(c, &parse::<RemoveDestinationRequest>(method, params)?, None)
        }
        other => Err(RpcError::not_found("method", other, &[])),
    }
}
