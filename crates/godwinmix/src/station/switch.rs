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

use super::direct::{outputs, INPUT_SOURCE};
use super::registry::MAIN;
use super::state::Station;
use super::{files, supervise};
use godwinmix_protocol::destination::StoredDestination;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::SwitchReport;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

const ASK: Duration = Duration::from_secs(5);
/// How long the outputs get to come back before the answer goes anyway.
const LIVE_WAIT: Duration = Duration::from_secs(30);

fn refuse(id: &str, why: String) -> RpcError {
    RpcError::not_in_state(why).with("show", id)
}

fn moving(st: &Station, id: &str) -> Vec<StoredDestination> {
    let records = st.registry.lock().get(id).map(|r| r.outputs.clone()).unwrap_or_default();
    outputs::stored(id, &records).into_iter().filter(|d| d.enabled).collect()
}

async fn handed(st: &Arc<Station>) {
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
async fn wait_live<F, Fut>(since: Instant, mut live: F) -> Option<u64>
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

fn report(compositing: bool, outputs: Vec<String>, gap_ms: Option<u64>, mut note: String) -> SwitchReport {
    if gap_ms.is_none() && !outputs.is_empty() {
        note.push_str(&format!("Not every output was live again within {} seconds; show.stats says how each is doing.", LIVE_WAIT.as_secs()));
    }
    let gap_ms = gap_ms.filter(|_| !outputs.is_empty());
    SwitchReport { compositing, outputs, gap_ms, note: note.trim().to_string() }
}

/// What stops a show going back to no compositing, if anything.
async fn blocker(st: &Arc<Station>, id: &str, known: &[String]) -> Result<Vec<String>, RpcError> {
    let sources = st.ask_show(id, "source.list", json!({}), ASK).await?;
    let rows = sources.get("sources").and_then(Value::as_array).or_else(|| sources.as_array()).cloned().unwrap_or_default();
    let others: Vec<String> = rows.iter().filter_map(|s| s["id"].as_str()).filter(|s| *s != INPUT_SOURCE).map(str::to_string).collect();
    if !others.is_empty() {
        let why = format!("show {id} has sources besides its input ({}), which a show without compositing cannot carry. Remove them first.", others.join(", "));
        return Err(refuse(id, why).with("sources", others));
    }
    let status = st.ask_show(id, "core.status", json!({}), ASK).await?;
    if let Some(scene) = status["scene"].as_str() {
        let why = format!("show {id} has the scene {scene} on programme. Take the input to programme first, so nothing on air changes.");
        return Err(refuse(id, why).with("scene", scene));
    }
    let outs = st.ask_show(id, "output.list", json!({}), ASK).await?;
    let ids: Vec<String> = outs.as_array().cloned().unwrap_or_default().iter().filter_map(|o| o["id"].as_str().map(str::to_string)).collect();
    let strangers: Vec<String> = ids.iter().filter(|o| !known.contains(o)).cloned().collect();
    if !strangers.is_empty() {
        let why = format!(
            "show {id} has outputs that were added inside it ({}), and their addresses are write only, so they cannot be moved. \
             Remove them and add them again with show.output.add once compositing is off.",
            strangers.join(", ")
        );
        return Err(refuse(id, why).with("outputs", strangers));
    }
    Ok(ids)
}

/// Compositing to direct.
pub async fn off(st: &Arc<Station>, id: &str) -> Result<SwitchReport, RpcError> {
    let r = st.registry.lock().get(id).cloned().ok_or_else(|| RpcError::not_found("show", id, &[]))?;
    if id == MAIN {
        return Err(refuse(id, "main is the show the station was started with and always composites. Add a show for the feed instead.".into()));
    }
    if r.input.is_none() {
        let why = format!("show {id} has no input to send on. Give it one first with show.set {{id: \"{id}\", input: {{uri: ...}}}}.");
        return Err(refuse(id, why).with("field", "input"));
    }
    if st.state_of(id) != Some(godwinmix_protocol::shows::ShowState::Running) {
        let why = format!("show {id} is not running, so the station cannot see its sources and outputs. Start it, then switch.");
        return Err(refuse(id, why));
    }
    let known: Vec<String> = r.outputs.iter().map(|o| o.id.clone()).collect();
    let ids = blocker(st, id, &known).await?;
    let stopped = Instant::now();
    for o in &ids {
        let _ = st.ask_show(id, "output.remove", json!({"id": o}), ASK).await;
    }
    {
        let mut reg = st.registry.lock();
        if let Some(r) = reg.get_mut(id) {
            r.compositing = false;
        }
        reg.save().map_err(|e| RpcError::internal(format!("saving the list of shows: {e:#}")))?;
    }
    handed(st).await;
    supervise::stop(st, id).await;
    if let Some(s) = st.direct.seen.lock().get_mut(id) {
        s.source_for = None;
    }
    let back: Vec<String> = moving(st, id).iter().map(|d| d.id.clone()).collect();
    let host = st.direct.plugins().is_some_and(|p| p.is_running(crate::channels::PLUGIN));
    if !host {
        st.announce(id);
        let note = "The ingest plugin, which sends the outputs of a show without compositing, is not running, so they wait for it.";
        return Ok(report(false, back, None, note.into()));
    }
    let gap_ms = wait_live(stopped, || async { st.direct.seen.lock().get(id).is_some_and(|s| s.all_live(&back)) }).await;
    st.announce(id);
    Ok(report(false, back, gap_ms, String::new()))
}
