//! `channel.*` under a station: the same work the single process core does
//! in `control/methods/channels.rs`, on the station's one registry of
//! channels, which the ingest plugin in the station serves.

use super::state::Station;
use crate::channels::Channels;
use crate::control::methods::channel_destinations as dest;
use godwinmix_protocol::channels::*;
use godwinmix_protocol::destination::{AddDestinationRequest, DestinationToOutputRequest, RemoveDestinationRequest, SetDestinationRequest};
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
    if method == "channel.destination.to_output" {
        return to_output(st, channels, parse(method, params)?).await;
    }
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

/// The programme of one show sent where a channel's destination points: the
/// key is read here, in the station, and handed to the show's `output.add`.
async fn to_output(st: &Arc<Station>, channels: Arc<Channels>, req: DestinationToOutputRequest) -> Result<Value, RpcError> {
    const ASK: std::time::Duration = std::time::Duration::from_secs(15);
    let show = req.show.clone().unwrap_or_else(|| st.first());
    let (c, (channel, wanted)) = (channels.clone(), (req.id.clone(), req.destination.clone()));
    let found = tokio::task::spawn_blocking(move || dest::to_output::find(&*c, &channel, &wanted))
        .await
        .map_err(|e| RpcError::internal(format!("reading the destination stopped: {e}")))??;
    let status = st.ask_show(&show, "core.status", Value::Null, ASK).await?;
    let taken: Vec<String> = status["outputs"].as_array().into_iter().flatten().filter_map(|o| o["id"].as_str().map(String::from)).collect();
    let output = st.ask_show(&show, "output.add", dest::to_output::output_params(&found, &taken), ASK).await?;
    if req.remove {
        let gone = RemoveDestinationRequest { id: req.id, destination: req.destination };
        tokio::task::spawn_blocking(move || dest::remove(&*channels, &gone, None))
            .await
            .map_err(|e| RpcError::internal(format!("taking the destination off the channel stopped: {e}")))??;
    }
    Ok(output)
}
