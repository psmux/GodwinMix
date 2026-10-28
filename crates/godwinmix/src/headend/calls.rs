//! `headend.*` as the station answers it: `headend.import` and
//! `headend.status` here, the headend's own setup in `native/calls.rs`.
//!
//! An import is the one `godwinmix headend` runs on its file, made once:
//! the config is read and imported by `godwinmix_flussonic`, its shows are
//! priced by `show.add_many`'s dry run (the default stops there), and then
//! each stream becomes a channel the station keeps, editable like any set
//! up on the headend screen. Nothing follows the text afterwards.

use super::methods::NOT_FOLLOWING;
use super::native::{compile, Native};
use crate::station::methods;
use crate::station::state::Station;
use godwinmix_flussonic::import::{self, ShowPlan};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::headend::{HeadendImportReport, HeadendImportRequest, HeadendStatus};
use godwinmix_protocol::scope::Token;
use godwinmix_protocol::shows::{ShowAddManyResult, ShowRefused};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::Arc;

pub async fn call(st: &Arc<Station>, token: &Token, method: &str, params: Value) -> Result<Value, RpcError> {
    if super::native::calls::METHODS.contains(&method) {
        return super::native::calls::call(st, method, params).await;
    }
    let out = match method {
        "headend.status" => serde_json::to_value(status(st)),
        "headend.import" => serde_json::to_value(run(st, token, params).await?),
        other => return Err(methods::no_such_method(other)),
    };
    out.map_err(|e| RpcError::internal(format!("encoding the answer: {e}")))
}

pub fn status(st: &Station) -> HeadendStatus {
    let Some(h) = st.headend.get().and_then(std::sync::Weak::upgrade) else {
        return HeadendStatus { note: NOT_FOLLOWING.into(), ..Default::default() };
    };
    let (listeners, log) = (h.addrs.clone(), h.log_path.as_ref().map(|p| p.display().to_string()));
    let Some(file) = &h.file else {
        let note = "This headend follows no Flussonic config: its channels are set up on the headend screen or with headend.channel.add, and Flussonic's URLs and API answer on the listeners.";
        return HeadendStatus { listeners, log, shows: h.shows().len(), note: note.into(), ..Default::default() };
    };
    let streams = h.doc.lock().config.streams.len();
    let shows = h.keep.running.lock().len();
    HeadendStatus {
        following: true,
        file: Some(file.display().to_string()),
        listeners,
        log,
        streams,
        shows,
        note: "The station applies every change to this file within a second. Edit the file, or PUT \
               to Flussonic's API on the listener, to change its streams. A stream edited on the \
               headend screen is kept by the station from then on."
            .into(),
    }
}

async fn run(st: &Arc<Station>, token: &Token, params: Value) -> Result<HeadendImportReport, RpcError> {
    let req: HeadendImportRequest = serde_json::from_value(if params.is_null() { json!({}) } else { params })
        .map_err(|e| RpcError::invalid_params(format!("headend.import could not read its params: {e}. Send {{\"text\": ...}} or {{\"path\": ...}}, and dry_run.")))?;
    let (from, text) = super::import_read::source(&req)?;
    let doc = super::import_read::parse(&from, &text)?;
    let made = import::import(&doc.config);
    if made.shows.is_empty() {
        return Err(RpcError::invalid_params(format!("{from} has no stream with an input, so there is nothing to make. A stream is a 'stream NAME {{ input URL; }}' block.")));
    }
    let dry_run = req.dry_run.unwrap_or(true);
    let mut result = make(st, token, &made.shows).await?;
    if !dry_run {
        keep(st, &from, &made.shows, &mut result).await?;
    }
    result.dry_run = dry_run;
    Ok(super::report::report(from, dry_run, made, result, status(st).file))
}

/// `show.add_many`'s dry run for the streams that are not channels or
/// shows already. One whose name is taken is refused here, since it would
/// be made again under another id.
async fn make(st: &Arc<Station>, token: &Token, plans: &[ShowPlan]) -> Result<ShowAddManyResult, RpcError> {
    let listed = methods::call(st, token, "show.list", json!({})).await?;
    let mut taken: BTreeSet<String> = listed["shows"].as_array().into_iter().flatten().flat_map(|s| [s["id"].as_str(), s["name"].as_str()]).flatten().map(str::to_lowercase).collect();
    taken.extend(Native::of(st).store.lock().channels.iter().map(|c| c.name.to_lowercase()));
    let mut fresh = Vec::new();
    let mut refused = Vec::new();
    for (index, p) in plans.iter().enumerate() {
        let id = crate::channels::keys::slug(&p.name);
        if !(taken.contains(&id) || taken.contains(&p.name.to_lowercase())) {
            fresh.push(index);
            continue;
        }
        let why = format!("there is already a channel or show {id}, made from this stream before. Remove it, or give the stream another name, and import again.");
        refused.push(ShowRefused { index, name: p.name.clone(), why, data: json!({"show": id}) });
    }
    let mut result = ShowAddManyResult { dry_run: true, ..Default::default() };
    if !fresh.is_empty() {
        let shows: Vec<Value> = fresh.iter().map(|&i| super::keep::show_of(&plans[i])).collect();
        let answer = methods::call(st, token, "show.add_many", json!({"dry_run": true, "shows": shows})).await?;
        result = serde_json::from_value(answer).map_err(|e| RpcError::internal(format!("reading show.add_many's answer: {e}")))?;
        // Its indexes are into the streams it was given; the report's are into all of them.
        for r in &mut result.refused {
            r.index = fresh[r.index];
        }
    }
    result.refused.extend(refused);
    result.refused.sort_by_key(|r| r.index);
    Ok(result)
}

/// Keep every stream the dry run passed as a channel, and run them. The
/// answer's `added` becomes the shows made.
async fn keep(st: &Arc<Station>, from: &str, plans: &[ShowPlan], result: &mut ShowAddManyResult) -> Result<(), RpcError> {
    let n = Native::of(st);
    let profiles = compile::all_profiles(&n.store.lock().profiles);
    let refused: BTreeSet<usize> = result.refused.iter().map(|r| r.index).collect();
    let origin = format!("import of {from}");
    let channels = plans.iter().enumerate().filter(|(i, _)| !refused.contains(i)).map(|(_, p)| compile::channel_of(p, &profiles, &origin)).collect();
    n.start();
    let change = super::native::calls::add_imported(&n, st, channels, &origin).await?;
    result.added = change.done.iter().filter_map(|name| n.keep.id_of(name)).collect();
    for r in change.refused {
        let index = plans.iter().position(|p| p.name == r.name).unwrap_or(r.index);
        result.refused.push(ShowRefused { index, ..r });
    }
    result.refused.sort_by_key(|r| r.index);
    Ok(())
}
