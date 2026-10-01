//! Turning compositing on and off for a show, with its outputs going on.
//!
//! On: the host stops sending the outputs and keeps the input open in its
//! hub; the station starts a show process, gives it the input as its one
//! source (read from the hub, so nothing is opened or decoded twice) and
//! adds the outputs to it. Off: the reverse, allowed only when the show has
//! that one source, no scene on programme, and no output the station did
//! not hand it (an address added inside a show is write only, so it cannot
//! be moved). Outputs move break then make, because a platform takes one
//! publisher per key; the gap is measured from the moment they stopped to
//! the moment every one is live again.

use super::direct::outputs;
use super::state::Station;
use super::{files, supervise};
use godwinmix_protocol::destination::StoredDestination;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::SwitchReport;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod off;

pub use off::off;

pub(super) const ASK: Duration = Duration::from_secs(5);
/// How long the outputs get to come back before the answer goes anyway.
const LIVE_WAIT: Duration = Duration::from_secs(30);

pub(super) fn refuse(id: &str, why: String) -> RpcError {
    RpcError::not_in_state(why).with("show", id)
}

pub(super) fn moving(st: &Station, id: &str) -> Vec<StoredDestination> {
    let records = st.registry.lock().get(id).map(|r| r.outputs.clone()).unwrap_or_default();
    outputs::stored(id, &records).into_iter().filter(|d| d.enabled).collect()
}

pub(super) async fn handed(st: &Arc<Station>) {
    let asked = st.direct.hand_over();
    let waiting = st.clone();
    let _ = tokio::task::spawn_blocking(move || waiting.direct.wait_handed(asked, Duration::from_secs(10))).await;
}

/// Direct to compositing.
pub async fn on(st: &Arc<Station>, id: &str) -> Result<SwitchReport, RpcError> {
    let (config, folder) = {
        let reg = st.registry.lock();
        let r = reg.get(id).ok_or_else(|| RpcError::not_found("show", id, &reg.ids()))?;
        (r.config.clone(), reg.folder_for(id))
    };
    let config = match config {
        Some(c) => c,
        None => files::fresh(&folder).map_err(|e| RpcError::internal(format!("making the show's folder {}: {e:#}", folder.display())))?,
    };
    let moved = moving(st, id);
    {
        let mut reg = st.registry.lock();
        if let Some(r) = reg.get_mut(id) {
            r.compositing = true;
            r.config = Some(config);
        }
        reg.save().map_err(|e| RpcError::internal(format!("saving the list of shows: {e:#}")))?;
    }
    handed(st).await;
    let stopped = Instant::now();
    supervise::start(st, id);
    st.addr_of(id).await?;
    super::direct::feed_source(st, id).await;
    let mut note = String::new();
    for d in &moved {
        let mut params = json!({"id": d.id, "uri": d.url(), "policy": d.policy()});
        if let Some(r) = &d.rendition {
            params["rendition"] = serde_json::to_value(r).unwrap_or_default();
        }
        if let Err(e) = st.ask_show(id, "output.add", params, ASK).await {
            note.push_str(&format!("{} was not added to the show: {}. ", d.id, e.message));
        }
    }
    let ids: Vec<String> = moved.iter().map(|d| d.id.clone()).collect();
    let gap_ms = wait_live(stopped, || shows_outputs_live(st, id, &ids)).await;
    st.announce(id);
    Ok(report(true, ids, gap_ms, note))
}

async fn shows_outputs_live(st: &Arc<Station>, id: &str, ids: &[String]) -> bool {
    let Ok(list) = st.ask_show(id, "output.list", json!({}), ASK).await else { return false };
    let rows = list.as_array().cloned().unwrap_or_default();
    ids.iter().all(|o| rows.iter().any(|r| r["id"] == o.as_str() && r["state"] == "live"))
}

/// Poll `live` every 100 ms until it holds, for at most [`LIVE_WAIT`].
pub(super) async fn wait_live<F, Fut>(since: Instant, mut live: F) -> Option<u64>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    while since.elapsed() < LIVE_WAIT {
        if live().await {
            return Some(since.elapsed().as_millis() as u64);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

pub(super) fn report(compositing: bool, outputs: Vec<String>, gap_ms: Option<u64>, mut note: String) -> SwitchReport {
    if gap_ms.is_none() && !outputs.is_empty() && note.is_empty() {
        note.push_str(&format!("Not every output was live again within {} seconds; show.stats says how each is doing.", LIVE_WAIT.as_secs()));
    }
    let gap_ms = gap_ms.filter(|_| !outputs.is_empty());
    SwitchReport { compositing, outputs, gap_ms, note: note.trim().to_string() }
}
