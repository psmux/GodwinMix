//! Removing channels, giving them back to the followed file, and the
//! profiles saved here.

use super::edit::refusal;
use super::{check, compile, follow, Native};
use crate::station::state::Station;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::headend::profile::builtins;
use godwinmix_protocol::headend::{ChannelChange, ProfileChange, ProfileRemoveRequest, TranscodeProfile};
use std::sync::Arc;

pub async fn remove(n: &Native, st: &Arc<Station>, names: Vec<String>) -> Result<ChannelChange, RpcError> {
    let _one = n.edits.lock().await;
    let (mut done, mut refused) = (Vec::new(), Vec::new());
    for (i, name) in names.iter().enumerate() {
        let here = n.store.lock().channel(name).is_some();
        if here {
            n.store.lock().channels.retain(|c| &c.name != name);
            done.push(name.clone());
        } else if let Some((file, _)) = follow::file_channel(st, n, name) {
            // Hidden from the file from now on, as an edit would be.
            follow::detach(st, n, name, &file);
            done.push(name.clone());
        } else {
            refused.push(refusal(i, name, format!("there is no channel {name}. headend.channel.list has every one.")));
        }
    }
    n.save().map_err(RpcError::internal)?;
    n.apply().await;
    follow::reapply(st).await;
    Ok(ChannelChange { done, refused, dry_run: false, restarted: Vec::new() })
}

/// Give streams taken over here back to the followed file.
pub async fn give_back(n: &Native, st: &Arc<Station>, names: Vec<String>) -> Result<ChannelChange, RpcError> {
    let _one = n.edits.lock().await;
    if follow::headend(st).is_none() {
        return Err(RpcError::not_in_state("this station follows no Flussonic config, so there is no file to give a channel back to."));
    }
    let (mut done, mut refused) = (Vec::new(), Vec::new());
    for (i, name) in names.iter().enumerate() {
        if follow::give_back(st, n, name) {
            done.push(name.clone());
        } else {
            refused.push(refusal(i, name, format!("{name} was not taken over from the followed file, so it already follows it, or was made here.")));
        }
    }
    n.save().map_err(RpcError::internal)?;
    n.apply().await;
    follow::reapply(st).await;
    Ok(ChannelChange { done, refused, dry_run: false, restarted: Vec::new() })
}

fn users(n: &Native, id: &str) -> Vec<String> {
    n.store.lock().channels.iter().filter(|c| c.profile == id).map(|c| c.name.clone()).collect()
}

pub async fn save_profile(n: &Native, mut p: TranscodeProfile) -> Result<ProfileChange, RpcError> {
    let _one = n.edits.lock().await;
    p.builtin = false;
    check::profile(&p, &builtins()).map_err(|e| RpcError::invalid_params(e).with("profile", p.id.clone()))?;
    {
        let mut s = n.store.lock();
        match s.profiles.iter_mut().find(|x| x.id == p.id) {
            Some(x) => *x = p.clone(),
            None => s.profiles.push(p.clone()),
        }
    }
    n.save().map_err(RpcError::internal)?;
    n.apply().await;
    Ok(ProfileChange { channels: users(n, &p.id), id: p.id })
}

pub async fn remove_profile(n: &Native, req: ProfileRemoveRequest) -> Result<ProfileChange, RpcError> {
    let _one = n.edits.lock().await;
    if !n.store.lock().profiles.iter().any(|p| p.id == req.id) {
        let saved: Vec<String> = n.store.lock().profiles.iter().map(|p| p.id.clone()).collect();
        return Err(RpcError::not_found("saved profile", &req.id, &saved));
    }
    let using = users(n, &req.id);
    if !using.is_empty() {
        let Some(to) = req.move_to.clone() else {
            return Err(RpcError::not_in_state(format!("{} takes {}. Send move_to with the profile they take instead, or change them first.", using.join(", "), req.id)).with("channels", using));
        };
        let left: Vec<TranscodeProfile> = compile::all_profiles(&n.store.lock().profiles).into_iter().filter(|p| p.id != req.id).collect();
        if !left.iter().any(|p| p.id == to) {
            return Err(RpcError::invalid_params(format!("there is no profile {to} to move the channels to.")).with("move_to", to));
        }
        for c in n.store.lock().channels.iter_mut().filter(|c| c.profile == req.id) {
            c.profile = to.clone();
        }
    }
    n.store.lock().profiles.retain(|p| p.id != req.id);
    n.save().map_err(RpcError::internal)?;
    n.apply().await;
    Ok(ProfileChange { id: req.id, channels: using })
}
