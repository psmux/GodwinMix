//! Compositing off: the checks that keep what is on air whole, then the
//! outputs handed back to the direct host.

use super::{handed, moving, refuse, report, wait_live, ASK};
use crate::station::direct::INPUT_SOURCE;
use crate::station::registry::MAIN;
use crate::station::state::Station;
use crate::station::supervise;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::SwitchReport;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;

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
    handed(st, id).await;
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
