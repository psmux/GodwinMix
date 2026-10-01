//! `show.stats`: health and numbers for many shows in one read, from what
//! the station already holds. Nothing is asked of a show or the host, so a
//! page can read two hundred shows every second.

use super::state::Station;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::{ShowStatsList, ShowStatsRequest};
use serde_json::Value;

/// `show.stats`: every show asked for, from what the station holds.
pub fn stats(st: &Station, req: ShowStatsRequest) -> Result<Value, RpcError> {
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
    let shows = ids
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
    Ok(serde_json::to_value(ShowStatsList { shows }).unwrap_or_default())
}
