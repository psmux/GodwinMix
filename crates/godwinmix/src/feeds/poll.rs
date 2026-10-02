//! A polled feed's task: fetch, read if it changed, write what changed,
//! sleep. One per feed, so a server that hangs holds only its own feed, and
//! only for as long as its timeout.

use super::fetch::{self, Validators};
use super::{bind, parse, Ctx, Feeds};
use godwinmix_protocol::feeds::FeedFormat;
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::Notify;

/// A body bigger than this is read on a blocking thread, so a 4 MB feed
/// does not hold a runtime worker that a client's call is waiting for.
const READ_INLINE: usize = 256 * 1024;

pub async fn run(feeds: Arc<Feeds>, ctx: Ctx, id: String, wake: Arc<Notify>) {
    let mut since = Validators::default();
    let mut last_hash: Option<u64> = None;
    loop {
        let Some(plan) = feeds.plan(&id) else { return };
        match fetch::get(&plan.address, &plan.headers, plan.timeout, &since).await {
            Err(e) => feeds.note_failure(&ctx, &id, e),
            Ok(got) => match got.body {
                None => unchanged(&feeds, &ctx, &id, true, None).await,
                Some(body) => {
                    let hash = hash(&body);
                    if last_hash == Some(hash) {
                        since = got.validators;
                        unchanged(&feeds, &ctx, &id, false, Some(body.len())).await;
                    } else {
                        let bytes = body.len();
                        match read(body, got.content_type, plan.format).await {
                            Ok(doc) => {
                                since = got.validators;
                                last_hash = Some(hash);
                                let changed = feeds.note_doc(&ctx, &id, doc, bytes);
                                bind::apply(&feeds, &ctx, &id, !changed).await;
                            }
                            Err(e) => {
                                since = Validators::default();
                                feeds.note_failure(&ctx, &id, format!("the body would not read: {e}"));
                            }
                        }
                    }
                }
            },
        }
        let wait = feeds.wait(&id, plan.interval);
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = wake.notified() => {}
        }
    }
}

/// Nothing new. A binding that failed last time is tried again, in case
/// what it writes to has come back.
async fn unchanged(feeds: &Arc<Feeds>, ctx: &Ctx, id: &str, not_modified: bool, bytes: Option<usize>) {
    feeds.note_same(ctx, id, not_modified, bytes);
    if feeds.any_failing(id) {
        bind::apply(feeds, ctx, id, true).await;
    }
}

pub async fn read(body: Vec<u8>, content_type: Option<String>, format: FeedFormat) -> Result<Value, String> {
    if body.len() <= READ_INLINE {
        return parse::read(&body, content_type.as_deref(), format).map(|(_, doc)| doc);
    }
    tokio::task::spawn_blocking(move || parse::read(&body, content_type.as_deref(), format).map(|(_, doc)| doc))
        .await
        .map_err(|e| format!("reading it stopped: {e}"))?
}

fn hash(body: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    body.hash(&mut h);
    h.finish()
}
