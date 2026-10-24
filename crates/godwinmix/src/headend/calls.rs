//! `headend.import` and `headend.status`, as the station answers them.
//!
//! An import is the one `godwinmix headend` runs on its file, made once:
//! the config is read and imported by `godwinmix_flussonic`, its shows go
//! to `show.add_many` (a dry run first, by default), and a stream that is
//! `disabled` or has a `title` is stopped or renamed after it is made, as
//! the headend's own apply does. Nothing follows the text afterwards.

use super::methods::NOT_FOLLOWING;
use crate::station::methods;
use crate::station::state::Station;
use godwinmix_flussonic::import::{self, ShowPlan};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::headend::{HeadendImportReport, HeadendImportRequest, HeadendStatus};
use godwinmix_protocol::scope::Token;
use godwinmix_protocol::shows::{ShowAddManyResult, ShowRefused};
use std::collections::BTreeSet;
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn call(st: &Arc<Station>, token: &Token, method: &str, params: Value) -> Result<Value, RpcError> {
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
    let streams = h.doc.lock().config.streams.len();
    let shows = h.running.lock().len();
    HeadendStatus {
        following: true,
        file: Some(h.file.display().to_string()),
        listeners: h.addrs.clone(),
        log: h.log_path.as_ref().map(|p| p.display().to_string()),
        streams,
        shows,
        note: "The station applies every change to this file within a second. Edit the file, or PUT \
               to Flussonic's API on the listener, to change its streams."
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
    let result = make(st, token, &made.shows, dry_run).await?;
    if !dry_run {
        settle(st, token, &made.shows, &result).await;
    }
    Ok(super::report::report(from, dry_run, made, result, status(st).file))
}

/// `show.add_many` for the streams that are not shows already. A stream
/// whose show is there from an import before is refused here, since
/// `show.add_many` would make it again under another id.
async fn make(st: &Arc<Station>, token: &Token, plans: &[ShowPlan], dry_run: bool) -> Result<ShowAddManyResult, RpcError> {
    let listed = methods::call(st, token, "show.list", json!({})).await?;
    let taken: BTreeSet<String> = listed["shows"].as_array().into_iter().flatten().flat_map(|s| [s["id"].as_str(), s["name"].as_str()]).flatten().map(str::to_lowercase).collect();
    let mut fresh = Vec::new();
    let mut refused = Vec::new();
    for (index, p) in plans.iter().enumerate() {
        let id = crate::channels::keys::slug(&p.name);
        if !(taken.contains(&id) || taken.contains(&p.name.to_lowercase())) {
            fresh.push(index);
            continue;
        }
        let why = format!("there is already a show {id}, made from this stream before. Remove it, or give the stream another name, and import again.");
        refused.push(ShowRefused { index, name: p.name.clone(), why, data: json!({"show": id}) });
    }
    let mut result = ShowAddManyResult { dry_run, ..Default::default() };
    if !fresh.is_empty() {
        let shows: Vec<Value> = fresh.iter().map(|&i| &plans[i]).map(|p| json!({"name": p.name, "compositing": false, "input": p.input, "outputs": p.outputs})).collect();
        let answer = methods::call(st, token, "show.add_many", json!({"dry_run": dry_run, "shows": shows})).await?;
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

/// Stop the streams that are `disabled` and name the ones with a `title`.
async fn settle(st: &Arc<Station>, token: &Token, plans: &[ShowPlan], result: &ShowAddManyResult) {
    let refused: Vec<usize> = result.refused.iter().map(|r| r.index).collect();
    let mut ids = result.added.iter();
    for (i, p) in plans.iter().enumerate() {
        if refused.contains(&i) {
            continue;
        }
        let Some(id) = ids.next() else { break };
        if p.disabled {
            if let Err(e) = methods::call(st, token, "show.stop", json!({"id": id})).await {
                tracing::warn!(stream = %p.name, error = %e.message, "headend import: a disabled stream was made and could not be stopped");
            }
        }
        if let Some(title) = &p.title {
            if let Err(e) = methods::call(st, token, "show.rename", json!({"id": id, "name": title})).await {
                tracing::warn!(stream = %p.name, error = %e.message, "headend import: a stream's title could not be given to its show");
            }
        }
    }
}

