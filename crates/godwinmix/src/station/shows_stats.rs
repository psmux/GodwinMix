//! `show.stats`: health and numbers for many shows in one read, from what
//! the station already holds. Nothing is asked of a show or the host, so a
//! page can read two hundred shows every second. A show that mixes is a
//! process of its own, and what it costs is read off that process (one `ps`
//! or `/proc` read for all of them, at most once a second).

use super::state::Station;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::{ShowStatsList, ShowStatsRequest, ShowWork};
use serde_json::Value;
use std::sync::Arc;

/// `show.stats`: every show asked for, from what the station holds.
pub async fn stats(st: &Arc<Station>, req: ShowStatsRequest) -> Result<Value, RpcError> {
    let all = st.registry.lock().ids();
    let ids = match req.ids {
        Some(ids) => {
            if let Some(missing) = ids.iter().find(|id| !all.contains(id)) {
                return Err(RpcError::not_found("show", missing, &all));
            }
            ids
        }
        None => all,
    };
    let wants = |f: &str| req.fields.as_ref().is_none_or(|fs| fs.iter().any(|x| x == f));
    let mut shows: Vec<_> = ids
        .iter()
        .map(|id| {
            let mut s = st.direct.stats_of(st, id);
            if !wants("input") {
                s.input = None;
            }
            if !wants("outputs") {
                s.outputs.clear();
            }
            s
        })
        .collect();
    if shows.iter().any(|s| s.work == ShowWork::Mix) {
        let children = super::usage::children(st).await;
        for s in shows.iter_mut().filter(|s| s.work == ShowWork::Mix) {
            s.cpu_millicores = children.show(&s.id);
        }
    }
    Ok(serde_json::to_value(ShowStatsList { shows }).unwrap_or_default())
}
