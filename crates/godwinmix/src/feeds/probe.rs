//! `feed.test`: fetch once, show what came back, and what a selection picks.
//!
//! This is what makes binding easy. A person or an agent asks for the
//! document with no `select`, reads `paths`, tries one, and sees the value a
//! binding would write before making the binding. Nothing is stored.

use super::state::Plan;
use super::value::{self, NoValue};
use super::{check, fetch, parse, preview, stream, Feeds};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::feeds::{FeedFormat, FeedSpec, FeedTestRequest, FeedTestResult};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;

pub async fn test(feeds: &Feeds, req: FeedTestRequest) -> Result<FeedTestResult, RpcError> {
    let started = Instant::now();
    let (format, doc, bytes) = match (&req.id, &req.address) {
        (Some(id), _) => {
            let spec = feeds.spec(id)?;
            match feeds.doc(id).filter(|_| !req.fresh) {
                Some(doc) => (spec.format, doc, feeds.status(id)?.bytes),
                None => once(&spec).await?,
            }
        }
        (None, Some(address)) => {
            let spec = FeedSpec {
                id: "test".into(),
                address: address.clone(),
                format: req.format.unwrap_or_default(),
                interval_s: None,
                timeout_s: req.timeout_s,
                headers: req.headers.clone(),
                paused: false,
            };
            check::address(address, spec.format)?;
            once(&spec).await?
        }
        (None, None) => {
            return Err(check::field("address", "feed.test needs a feed: give id for one that exists, or address for one to try.".into()))
        }
    };
    let mut result = FeedTestResult {
        format,
        bytes,
        took_ms: started.elapsed().as_millis() as u64,
        keys: preview::keys(&doc),
        preview: preview::cut(&doc),
        paths: preview::paths(&doc),
        selected: None,
        value: None,
    };
    let selection = req.selection();
    if !selection.select.is_empty() || selection.template.is_some() {
        let (picked, out) = value::compute(&doc, &selection).map_err(|e| no_value(e, &doc))?;
        result.selected = Some(preview::cut(&picked));
        result.value = Some(out);
    }
    Ok(result)
}

/// A selection that picked nothing: where it stopped, what was there, the
/// top of the document and the paths that would have worked.
pub fn no_value(e: NoValue, doc: &Value) -> RpcError {
    let top = preview::keys(doc);
    let mut message = e.message.clone();
    if !top.is_empty() {
        message.push_str(&format!(" The top of the document has: {}.", top.join(", ")));
    }
    message.push_str(" Pick a path from data.paths, or call feed.test with no select to see the whole document.");
    let paths: Vec<String> = preview::paths(doc).into_iter().take(40).map(|p| p.path).collect();
    let mut err = check::field("select", message).with("top_level_keys", json!(top)).with("paths", json!(paths));
    if let Some(m) = e.miss {
        err = err.with("stopped_at", m.at).with("keys_there", json!(m.keys));
    }
    err
}

/// One read of a feed, whatever kind it is.
async fn once(spec: &FeedSpec) -> Result<(FeedFormat, Arc<Value>, u64), RpcError> {
    let plan = Plan {
        address: spec.address.clone(),
        headers: spec.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        format: spec.format,
        interval: check::interval(spec),
        timeout: check::timeout(spec),
    };
    let unread = |e: String| RpcError::not_in_state(format!("the feed could not be read: {e}.")).with("retryable", true);
    if check::kind(spec) != check::Kind::Polled {
        let text = stream::first(plan).await.map_err(unread)?;
        let len = text.len() as u64;
        return Ok((FeedFormat::Json, Arc::new(parse::message(&text)), len));
    }
    let got = fetch::get(&plan.address, &plan.headers, plan.timeout, &Default::default()).await.map_err(unread)?;
    let body = got.body.unwrap_or_default();
    let (format, doc) = parse::read(&body, got.content_type.as_deref(), spec.format)
        .map_err(|e| unread(format!("it came back, {} bytes, but {e}. Give format to say what it is", body.len())))?;
    Ok((format, Arc::new(doc), body.len() as u64))
}
