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
use godwinmix_flussonic::Document;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::headend::{HeadendImportReport, HeadendImportRequest, HeadendStatus};
use godwinmix_protocol::scope::Token;
use godwinmix_protocol::shows::ShowAddManyResult;
use serde_json::{json, Value};
use std::sync::Arc;

/// The largest config read, in bytes. A 200 stream config is about 60 kB.
const MOST: u64 = 4 << 20;

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
    HeadendStatus {
        following: true,
        file: Some(h.file.display().to_string()),
        listeners: h.addrs.clone(),
        log: h.log_path.as_ref().map(|p| p.display().to_string()),
        streams: h.doc.lock().config.streams.len(),
        shows: h.running.lock().len(),
        note: "The station applies every change to this file within a second. Edit the file, or PUT \
               to Flussonic's API on the listener, to change its streams."
            .into(),
    }
}

/// The config's text and where it came from.
fn source(req: &HeadendImportRequest) -> Result<(String, String), RpcError> {
    match (req.text.as_deref(), req.path.as_deref()) {
        (Some(t), None) => Ok(("pasted text".into(), t.to_string())),
        (None, Some(p)) => {
            let len = std::fs::metadata(p).map(|m| m.len()).map_err(|e| unreadable(p, &e))?;
            if len > MOST {
                return Err(RpcError::invalid_params(format!("{p} is {len} bytes, more than a Flussonic config is. Check the path names the config.")).with("path", p));
            }
            Ok((p.to_string(), std::fs::read_to_string(p).map_err(|e| unreadable(p, &e))?))
        }
        _ => Err(RpcError::invalid_params("headend.import takes the config as text, or a path on the station's machine: send one of the two.")),
    }
}

fn unreadable(path: &str, e: &std::io::Error) -> RpcError {
    RpcError::invalid_params(format!("the station could not read {path}: {e}. Give a path on the station's machine that it may read, or paste the text.")).with("path", path)
}

fn parse(from: &str, text: &str) -> Result<Document, RpcError> {
    Document::parse(text).map_err(|e| {
        let line = match &e {
            godwinmix_flussonic::Error::Syntax { line, .. } | godwinmix_flussonic::Error::Invalid { line, .. } => *line,
            godwinmix_flussonic::Error::Edit(_) => 0,
        };
        RpcError::invalid_params(format!("{from} is not a config Flussonic would read, {e}. Fix that line and import again.")).with("line", line)
    })
}

async fn run(st: &Arc<Station>, token: &Token, params: Value) -> Result<HeadendImportReport, RpcError> {
    let req: HeadendImportRequest = serde_json::from_value(if params.is_null() { json!({}) } else { params })
        .map_err(|e| RpcError::invalid_params(format!("headend.import could not read its params: {e}. Send {{\"text\": ...}} or {{\"path\": ...}}, and dry_run.")))?;
    let (from, text) = source(&req)?;
    let doc = parse(&from, &text)?;
    let made = import::import(&doc.config);
    if made.shows.is_empty() {
        return Err(RpcError::invalid_params(format!("{from} has no stream with an input, so there is nothing to make. A stream is a 'stream NAME {{ input URL; }}' block.")));
    }
    let dry_run = req.dry_run.unwrap_or(true);
    let shows: Vec<Value> = made.shows.iter().map(|p| json!({"name": p.name, "compositing": false, "input": p.input, "outputs": p.outputs})).collect();
    let answer = methods::call(st, token, "show.add_many", json!({"dry_run": dry_run, "shows": shows})).await?;
    let result: ShowAddManyResult = serde_json::from_value(answer).map_err(|e| RpcError::internal(format!("reading show.add_many's answer: {e}")))?;
    if !dry_run {
        settle(st, token, &made.shows, &result).await;
    }
    Ok(super::report::report(from, dry_run, made, result, status(st).file))
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

