//! Adding, changing and removing channels. Every change is checked, priced
//! by `show.add_many`'s dry run where it makes a show, written to
//! `headend.json`, and then run by the keeper, which restarts only the
//! channels whose input or conversion moved.

use super::{check, compile, follow, Native};
use crate::headend::keep::{show_of, Outcome};
use crate::station::methods;
use crate::station::state::Station;
use godwinmix_flussonic::import::ShowPlan;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::headend::{ChannelChange, HeadendChannel};
use godwinmix_protocol::scope::Token;
use godwinmix_protocol::shows::ShowRefused;
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;

pub fn refusal(index: usize, name: &str, why: impl Into<String>) -> ShowRefused {
    ShowRefused { index, name: name.to_string(), why: why.into(), data: json!({"channel": name}) }
}

/// Every name in use: the channels here and the file's own streams.
fn taken(st: &Station, n: &Native) -> BTreeSet<String> {
    let mine: BTreeSet<String> = n.store.lock().channels.iter().map(|c| c.name.clone()).collect();
    let theirs = follow::file_plans(st, n).map(|(_, p)| p.into_iter().map(|p| p.name).collect::<Vec<_>>()).unwrap_or_default();
    mine.into_iter().chain(theirs).collect()
}

/// `show.add_many`'s dry run for `plans`, each under `as_name`: the reasons
/// it would refuse them, by place.
async fn priced(st: &Arc<Station>, plans: &[ShowPlan], as_name: impl Fn(&ShowPlan) -> String) -> Result<Vec<(usize, String)>, RpcError> {
    if plans.is_empty() {
        return Ok(Vec::new());
    }
    let shows: Vec<_> = plans.iter().map(|p| {
        let mut v = show_of(p);
        v["name"] = json!(as_name(p));
        v
    }).collect();
    let answer = methods::call(st, &Token::open(), "show.add_many", json!({"dry_run": true, "shows": shows})).await?;
    Ok(answer["refused"].as_array().into_iter().flatten().map(|r| (r["index"].as_u64().unwrap_or(0) as usize, r["why"].as_str().unwrap_or_default().to_string())).collect())
}

/// What the keeper did, as the answer says it.
fn said(out: &Outcome, wanted: &[String], change: &mut ChangeParts) {
    for (name, why) in &out.refused {
        if let Some(i) = wanted.iter().position(|w| w == name) {
            change.refused.push(refusal(i, name, format!("kept, and tried again every 30 s: {why}")));
        }
    }
    change.restarted.extend(out.restarted.iter().filter(|n| wanted.contains(n)).cloned());
}

#[derive(Default)]
pub struct ChangeParts {
    pub refused: Vec<ShowRefused>,
    pub restarted: Vec<String>,
}

pub async fn add(n: &Native, st: &Arc<Station>, channels: Vec<HeadendChannel>, dry_run: bool, origin: &str) -> Result<ChannelChange, RpcError> {
    let _one = n.edits.lock().await;
    let profiles = compile::all_profiles(&n.store.lock().profiles);
    let mut used = taken(st, n);
    let mut parts = ChangeParts::default();
    let mut fit: Vec<(usize, HeadendChannel, ShowPlan)> = Vec::new();
    for (i, mut c) in channels.into_iter().enumerate() {
        let checked = check::channel(&c, &profiles).and_then(|_| compile::plan(&c, &profiles));
        match checked {
            Err(why) => parts.refused.push(refusal(i, &c.name, why)),
            Ok(_) if !used.insert(c.name.clone()) => parts.refused.push(refusal(i, &c.name, format!("there is already a channel {}. Give this one another name, or change that one.", c.name))),
            Ok(p) => {
                c.origin = Some(c.origin.clone().unwrap_or_else(|| origin.to_string()));
                fit.push((i, c, p));
            }
        }
    }
    let plans: Vec<ShowPlan> = fit.iter().map(|(_, _, p)| p.clone()).collect();
    let no = priced(st, &plans, |p| p.name.clone()).await?;
    for (k, why) in &no {
        let (i, c, _) = &fit[*k];
        parts.refused.push(refusal(*i, &c.name, why.clone()));
    }
    let made: Vec<HeadendChannel> = fit.into_iter().enumerate().filter(|(k, _)| !no.iter().any(|(r, _)| r == k)).map(|(_, (_, c, _))| c).collect();
    let names: Vec<String> = made.iter().map(|c| c.name.clone()).collect();
    if !dry_run && !made.is_empty() {
        made.into_iter().for_each(|c| n.store.lock().put(c));
        n.save().map_err(RpcError::internal)?;
        let out = n.apply().await;
        said(&out, &names, &mut parts);
    }
    parts.refused.sort_by_key(|r| r.index);
    Ok(ChannelChange { done: names, refused: parts.refused, dry_run, restarted: Vec::new() })
}

/// The channel `name` as it is now, and the followed file it is a stream
/// of, when it is still the file's.
pub fn current(st: &Station, n: &Native, name: &str) -> Result<(HeadendChannel, Option<String>), RpcError> {
    if let Some(c) = n.store.lock().channel(name) {
        return Ok((c.clone(), None));
    }
    match follow::file_channel(st, n, name) {
        Some((file, c)) => Ok((c, Some(file))),
        None => {
            let names: Vec<String> = n.store.lock().channels.iter().map(|c| c.name.clone()).collect();
            Err(RpcError::not_found("channel", name, &names))
        }
    }
}

/// One channel to replace: its name now, the file it is still a stream
/// of, and what it becomes.
pub struct Change {
    pub was: String,
    pub file: Option<String>,
    pub to: HeadendChannel,
}

/// Replace channels whole, as checked, and run them. `pairs` is (the name
/// now, what it becomes).
pub async fn replace(n: &Native, st: &Arc<Station>, pairs: Vec<Change>) -> Result<ChannelChange, RpcError> {
    let _one = n.edits.lock().await;
    let profiles = compile::all_profiles(&n.store.lock().profiles);
    let used = taken(st, n);
    let mut parts = ChangeParts::default();
    let mut fit: Vec<(usize, Change, ShowPlan)> = Vec::new();
    for (i, ch) in pairs.into_iter().enumerate() {
        let checked = check::channel(&ch.to, &profiles).and_then(|_| compile::plan(&ch.to, &profiles));
        match checked {
            Err(why) => parts.refused.push(refusal(i, &ch.was, why)),
            Ok(_) if ch.to.name != ch.was && used.contains(&ch.to.name) => parts.refused.push(refusal(i, &ch.was, format!("there is already a channel {}. Pick a name no channel has.", ch.to.name))),
            Ok(p) => fit.push((i, ch, p)),
        }
    }
    let plans: Vec<ShowPlan> = fit.iter().map(|(_, _, p)| p.clone()).collect();
    let no = priced(st, &plans, |p| format!("{}-check", p.name)).await?;
    for (k, why) in &no {
        parts.refused.push(refusal(fit[*k].0, &fit[*k].1.was, why.clone()));
    }
    let keep: Vec<Change> = fit.into_iter().enumerate().filter(|(k, _)| !no.iter().any(|(r, _)| r == k)).map(|(_, (_, ch, _))| ch).collect();
    let names: Vec<String> = keep.iter().map(|ch| ch.to.name.clone()).collect();
    for ch in keep {
        if let Some(file) = &ch.file {
            follow::detach(st, n, &ch.was, file);
        }
        let mut s = n.store.lock();
        if ch.was != ch.to.name {
            s.channels.retain(|x| x.name != ch.was);
        }
        s.put(ch.to);
    }
    n.save().map_err(RpcError::internal)?;
    let out = n.apply().await;
    said(&out, &names, &mut parts);
    follow::reapply(st).await;
    Ok(ChannelChange { done: names, refused: parts.refused, dry_run: false, restarted: parts.restarted })
}
