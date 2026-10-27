//! The station's answer to `gpu.status`, with the channels on each card,
//! and to `gpu.rebalance`.
//!
//! Both the direct shows (the headend's channels) and the ingest channels
//! have renditions placed on cards, each by its own planner against the one
//! governor, so both are asked and their answers put together.

use super::state::Station;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::machine::{GpuRebalance, GpuRebalanceRequest, GpuStatus, GpuStatusRequest};
use serde_json::Value;
use std::collections::BTreeMap;

/// Every show and channel on a card, by card.
pub fn on_cards(st: &Station) -> BTreeMap<u32, Vec<String>> {
    let mut out: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut placed = st.direct.transcode.placements();
    if let Some(ch) = st.channels.get() {
        placed.extend(ch.transcode().placements());
    }
    for (id, card) in placed {
        out.entry(card).or_default().push(id);
    }
    out
}

/// `gpu.status`, each card with what runs on it.
pub async fn status(st: &Station, params: Value) -> Result<Value, RpcError> {
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    let req: GpuStatusRequest = serde_json::from_value(params)
        .map_err(|e| RpcError::invalid_params(format!("gpu.status could not read its params: {e}. Send {{}} or {{\"since_ms\": <unix ms>}}.")))?;
    let mut status: GpuStatus = crate::machine::gpu_status(req.since_ms).await;
    let mut on = on_cards(st);
    for d in &mut status.gpus {
        d.channels = on.remove(&d.index).unwrap_or_default();
    }
    serde_json::to_value(status).map_err(|e| RpcError::internal(format!("encoding the answer: {e}")))
}

/// `gpu.rebalance`: move the fewest channels placed by `auto` that even
/// the cards out, and plan again so the moved ones start on their new card.
pub fn rebalance(st: &Station, params: Value) -> Result<Value, RpcError> {
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    let req: GpuRebalanceRequest = serde_json::from_value(params)
        .map_err(|e| RpcError::invalid_params(format!("gpu.rebalance could not read its params: {e}. Send {{}}, or {{\"only\": [ids], \"dry_run\": true}}.")))?;
    let out = rebalance_these(st, req.only.as_deref(), req.dry_run);
    if out.gpus.len() < 2 {
        return Err(RpcError::not_in_state(format!(
            "this machine has {} NVIDIA card{} a channel can be put on, so there is nothing to even out.",
            out.gpus.len(),
            if out.gpus.len() == 1 { "" } else { "s" }
        ))
        .with("gpus", out.gpus));
    }
    serde_json::to_value(out).map_err(|e| RpcError::internal(format!("encoding the answer: {e}")))
}

/// The same for code that already has the ids, such as a headend bulk edit.
pub fn rebalance_these(st: &Station, only: Option<&[String]>, dry_run: bool) -> GpuRebalance {
    let mut out = st.direct.transcode.rebalance(only, dry_run);
    if !dry_run && !out.moved.is_empty() {
        st.direct.hand_over();
    }
    if let Some(ch) = st.channels.get() {
        let more = ch.rebalance_gpus(only, dry_run);
        out.kept += more.kept;
        out.moved.extend(more.moved);
    }
    out
}
