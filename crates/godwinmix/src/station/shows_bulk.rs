//! `show.add_many` and `show.remove_many`.
//!
//! A batch is checked whole before anything is made: every show is
//! prepared (name, input, outputs by the rules) and every rendition is
//! priced by the planner against the governor's free room, taking no
//! ticket. A dry run, the default, stops there and says what would happen.
//! Otherwise each show that was prepared and fits is made, whole; one that
//! does not is refused with why and its data. The table is handed to the
//! host once for the whole batch.

use super::shows_direct::{make_direct, prepare, Prepared};
use super::state::Station;
use godwinmix_govern::headroom::short;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::rendition::Cost;
use godwinmix_protocol::shows::*;
use serde_json::{json, Value};
use std::sync::Arc;

/// The most shows one call may carry.
const MOST: usize = 1000;

struct Priced {
    index: usize,
    add: ShowAdd,
    prepared: Prepared,
    cost: Cost,
}

fn refusal(index: usize, name: &str, e: &RpcError) -> ShowRefused {
    ShowRefused { index, name: name.to_string(), why: e.message.clone(), data: e.data.clone() }
}

/// Check and price the whole batch. Nothing is made and nothing is held.
fn check(st: &Station, shows: &[ShowAdd]) -> (Vec<Priced>, Vec<ShowRefused>) {
    let (mut ok, mut refused, mut taken) = (Vec::new(), Vec::new(), Vec::<String>::new());
    for (index, add) in shows.iter().enumerate() {
        let req = ShowAddRequest::from(add.clone());
        let prepared = prepare(&st.registry.lock(), &req, &taken);
        let prepared = match prepared {
            Ok(p) => p,
            Err(e) => {
                refused.push(refusal(index, &add.name, &e));
                continue;
            }
        };
        match st.direct.price(&prepared.outputs) {
            Ok(cost) => {
                taken.push(prepared.id.clone());
                ok.push(Priced { index, add: add.clone(), prepared, cost });
            }
            Err(no) => {
                let e = RpcError::invalid_params(format!("{}: {}", add.name, no.message)).with("refusal", serde_json::to_value(&no).unwrap_or_default());
                refused.push(refusal(index, &add.name, &e));
            }
        }
    }
    (ok, refused)
}

pub async fn add_many(st: &Arc<Station>, req: ShowAddManyRequest) -> Result<Value, RpcError> {
    if req.shows.len() > MOST {
        let msg = format!("{} shows in one call is more than {MOST}. Send them in batches of {MOST} or fewer.", req.shows.len());
        return Err(RpcError::invalid_params(msg).with("field", "shows").with("most", MOST as u64));
    }
    let dry_run = req.dry_run.unwrap_or(true);
    let (priced, mut refused) = check(st, &req.shows);
    let have = st.render.governor().headroom(None);
    let (_, label) = crate::channels::transcode::assumed_input();
    let mut cost = Cost::default();
    let mut fitting = Vec::new();
    for p in priced {
        let with = cost.plus(p.cost);
        if short(&with, &have).is_empty() {
            cost = with;
            fitting.push(p);
            continue;
        }
        let e = RpcError::not_in_state(format!(
            "{} would take this machine past what it has free: its renditions need {} millicores and {} are left after the shows before it.",
            p.add.name,
            p.cost.cpu_millicores,
            have.cpu_millicores.saturating_sub(cost.cpu_millicores)
        ))
        .with("alarm", "governor-refused")
        .with("need", serde_json::to_value(p.cost).unwrap_or_default())
        .with("have", serde_json::to_value(have).unwrap_or_default());
        refused.push(refusal(p.index, &p.add.name, &e));
    }
    let fits = refused.is_empty();
    let mut added = Vec::new();
    for p in fitting {
        if dry_run {
            added.push(p.prepared.id);
            continue;
        }
        match apply(st, p.add, p.prepared).await {
            Ok(id) => added.push(id),
            Err((name, e)) => refused.push(refusal(p.index, &name, &e)),
        }
    }
    refused.sort_by_key(|r| r.index);
    let plan = BulkPlan { cost, have, fits, assumed_input: label.to_string() };
    Ok(serde_json::to_value(ShowAddManyResult { added, refused, plan, dry_run }).unwrap_or_default())
}

/// Make one show of the batch.
async fn apply(st: &Arc<Station>, add: ShowAdd, p: Prepared) -> Result<String, (String, RpcError)> {
    let name = add.name.clone();
    if !p.compositing {
        return make_direct(st, p).map_err(|e| (name, e));
    }
    let made = super::shows_api::add(st, add.into()).await.map_err(|e| (name, e))?;
    Ok(made["id"].as_str().unwrap_or_default().to_string())
}

pub async fn remove_many(st: &Arc<Station>, req: ShowRemoveManyRequest) -> Result<Value, RpcError> {
    let (mut removed, mut refused) = (Vec::new(), Vec::new());
    for (index, id) in req.ids.iter().enumerate() {
        match super::shows_api::remove(st, id).await {
            Ok(_) => removed.push(id.clone()),
            Err(e) => refused.push(refusal(index, id, &e)),
        }
    }
    Ok(json!(ShowRemoveManyResult { removed, refused }))
}
