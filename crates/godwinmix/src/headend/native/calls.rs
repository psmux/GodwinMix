//! `headend.channel.*`, `headend.profile.*`, `headend.scan` and
//! `headend.export`, as the station answers them.

use super::edit::{self, Change};
use super::{compile, follow, ops, Native};
use crate::station::state::Station;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::headend::{
    ChannelAddRequest, ChannelBulkRequest, ChannelEntry, ChannelNames, ChannelUpdateRequest, HeadendEncoder, HeadendSetup, ProfileRemoveRequest, ProfileSaveRequest, ScanRequest,
};
use godwinmix_protocol::headend::profile::CUSTOM;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::sync::Arc;

/// The methods answered here.
pub const METHODS: &[&str] = &[
    "headend.channel.list",
    "headend.channel.add",
    "headend.channel.update",
    "headend.channel.bulk",
    "headend.channel.remove",
    "headend.channel.follow",
    "headend.profile.save",
    "headend.profile.remove",
    "headend.scan",
    "headend.export",
];

fn read<T: DeserializeOwned>(method: &str, params: Value) -> Result<T, RpcError> {
    let params = if params.is_null() { json!({}) } else { params };
    serde_json::from_value(params).map_err(|e| RpcError::invalid_params(format!("{method} could not read its params: {e}. core.api has its params schema.")))
}

fn answer<T: serde::Serialize>(v: T) -> Result<Value, RpcError> {
    serde_json::to_value(v).map_err(|e| RpcError::internal(format!("encoding the answer: {e}")))
}

pub async fn call(st: &Arc<Station>, method: &str, params: Value) -> Result<Value, RpcError> {
    let n = Native::of(st);
    n.start();
    match method {
        "headend.channel.list" => answer(list(st, &n)),
        "headend.channel.add" => {
            let req: ChannelAddRequest = read(method, params)?;
            answer(edit::add(&n, st, req.channels, req.dry_run, "added here").await?)
        }
        "headend.channel.update" => {
            let req: ChannelUpdateRequest = read(method, params)?;
            let (now, file) = edit::current(st, &n, &req.name)?;
            let mut to = req.channel;
            to.origin = now.origin;
            answer(edit::replace(&n, st, vec![Change { was: req.name, file, to }]).await?)
        }
        "headend.channel.bulk" => answer(bulk(st, &n, read(method, params)?).await?),
        "headend.channel.remove" => answer(ops::remove(&n, st, read::<ChannelNames>(method, params)?.names).await?),
        "headend.channel.follow" => answer(ops::give_back(&n, st, read::<ChannelNames>(method, params)?.names).await?),
        "headend.profile.save" => answer(ops::save_profile(&n, read::<ProfileSaveRequest>(method, params)?.profile).await?),
        "headend.profile.remove" => answer(ops::remove_profile(&n, read::<ProfileRemoveRequest>(method, params)?).await?),
        "headend.scan" => answer(super::scan::scan(st, &n, read::<ScanRequest>(method, params)?).await?),
        "headend.export" => answer(super::export::text(&n.store.lock())),
        other => Err(crate::station::methods::no_such_method(other)),
    }
}

/// Every channel, the station's then the file's, with every profile and
/// the encoders the machine has.
pub fn list(st: &Station, n: &Native) -> HeadendSetup {
    let s = n.store.lock().clone();
    let profiles = compile::all_profiles(&s.profiles);
    let mut channels: Vec<ChannelEntry> = s
        .channels
        .iter()
        .map(|c| ChannelEntry { channel: c.clone(), show: n.keep.id_of(&c.name), follows: None, detached_from: s.detached.get(&c.name).cloned() })
        .collect();
    let file = follow::headend(st).map(|h| h.file_text());
    if let (Some(h), Some((f, plans))) = (follow::headend(st), follow::file_plans(st, n)) {
        for p in plans {
            let c = compile::channel_of(&p, &profiles, &f);
            channels.push(ChannelEntry { show: h.keep.id_of(&p.name), channel: c, follows: Some(f.clone()), detached_from: None });
        }
    }
    let mut encoders: Vec<HeadendEncoder> = st.direct.transcode.slots().into_iter().map(|e| HeadendEncoder { codec: format!("{:?}", e.codec).to_lowercase(), on: if e.hardware { "gpu" } else { "cpu" }.into(), id: e.id }).collect();
    encoders.sort_by_key(|e| e.on != "gpu");
    HeadendSetup { channels, profiles, encoders, following: file }
}

async fn bulk(st: &Arc<Station>, n: &Native, req: ChannelBulkRequest) -> Result<godwinmix_protocol::headend::ChannelChange, RpcError> {
    if req.profile.as_deref() == Some(CUSTOM) && req.custom.is_none() {
        return Err(RpcError::invalid_params("profile custom needs custom: the video and audio every channel takes."));
    }
    let mut changes = Vec::new();
    let mut missing = Vec::new();
    for name in &req.names {
        let Ok((mut to, file)) = edit::current(st, n, name) else {
            missing.push(name.clone());
            continue;
        };
        if let Some(p) = &req.profile {
            to.profile = p.clone();
            to.custom = if p == CUSTOM { req.custom.clone() } else { None };
        }
        if let Some(on) = req.enabled {
            to.enabled = on;
        }
        if let Some(card) = &req.interface {
            to.input.interface = Some(card.trim().to_string()).filter(|c| !c.is_empty());
        }
        changes.push(Change { was: name.clone(), file, to });
    }
    let mut out = edit::replace(n, st, changes).await?;
    for name in missing {
        let i = req.names.iter().position(|x| *x == name).unwrap_or(0);
        out.refused.push(edit::refusal(i, &name, format!("there is no channel {name}.")));
    }
    Ok(out)
}

/// `headend.import`'s channels, kept as the import made them.
pub async fn add_imported(n: &Native, st: &Arc<Station>, channels: Vec<godwinmix_protocol::headend::HeadendChannel>, origin: &str) -> Result<godwinmix_protocol::headend::ChannelChange, RpcError> {
    edit::add(n, st, channels, false, origin).await
}
