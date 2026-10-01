//! Shows without compositing, and the wave 4 properties of every show:
//! checking what a new show asks for, making it whole, and its outputs.

use super::direct::{self, outputs};
use super::registry::{Record, Registry};
use super::state::Station;
use godwinmix_protocol::destination::StoredDestination;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::*;
use serde_json::Value;
use std::sync::Arc;

/// Schemes an input may have. `channel:<app>/<stream>` is a channel's.
const INPUTS: [&str; 11] = ["udp", "rtp", "srt", "rtmp", "rtmps", "rtsp", "http", "https", "file", "rist", "hls"];

/// A new show, checked and ready to be made.
pub struct Prepared {
    pub id: String,
    pub name: String,
    pub compositing: bool,
    pub input: Option<InputSpec>,
    pub outputs: Vec<StoredDestination>,
}

/// Is this an input the direct host can open?
pub fn check_input(input: &InputSpec) -> Result<(), RpcError> {
    let uri = input.uri.trim();
    let ok = match uri.split_once("://") {
        Some((scheme, rest)) => INPUTS.contains(&scheme.to_ascii_lowercase().as_str()) && !rest.is_empty(),
        None => uri.strip_prefix("channel:").is_some_and(|s| s.split_once('/').is_some_and(|(a, b)| !a.is_empty() && !b.is_empty())),
    };
    if !ok {
        let msg = format!(
            "\"{uri}\" is not an input this machine can open. Give an address such as udp://@239.1.1.1:5000, \
             srt://host:9000, rtmp://host/app/key, rtsp://camera/stream, https://host/live.m3u8 or \
             file:///clip.ts, or a channel's stream as channel:<app>/<stream>."
        );
        return Err(RpcError::invalid_params(msg).with("field", "input.uri").with("schemes", INPUTS.to_vec()));
    }
    match &input.backup {
        Some(b) if b.backup.is_some() => Err(RpcError::invalid_params("a backup input has no backup of its own").with("field", "input.backup.backup")),
        Some(b) => check_input(b).map_err(|e| e.with("backup", true)),
        None => Ok(()),
    }
}

/// Check one new show against the list and the ids a batch already took.
pub fn prepare(reg: &Registry, req: &ShowAddRequest, taken: &[String]) -> Result<Prepared, RpcError> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(RpcError::invalid_params("a show needs a name, such as \"Second room\"").with("field", "name"));
    }
    let compositing = req.compositing.unwrap_or(true);
    if let Some(input) = &req.input {
        check_input(input)?;
    }
    if !compositing && req.input.is_none() {
        let msg = "a show without compositing sends one input to its outputs, so it needs `input`, such as {\"uri\": \"udp://@239.1.1.1:5000\"}.";
        return Err(RpcError::invalid_params(msg).with("field", "input"));
    }
    if compositing && !req.outputs.is_empty() {
        let msg = "a show that composites keeps its outputs inside it: add them with output.add and ?show=<id> once it runs, or make it with compositing false.";
        return Err(RpcError::invalid_params(msg).with("field", "outputs"));
    }
    let id = free_id(reg, &name, taken);
    let mut list = Vec::new();
    for (i, spec) in req.outputs.iter().enumerate() {
        direct::add_output(&mut list, &id, spec).map_err(|e| e.with("output_index", i as u64))?;
    }
    Ok(Prepared { id, name, compositing, input: req.input.clone(), outputs: list })
}

fn free_id(reg: &Registry, name: &str, taken: &[String]) -> String {
    let base = reg.free_id(name);
    let stem = base.clone();
    std::iter::once(base)
        .chain((2..).map(|n| format!("{stem}-{n}")))
        .find(|id| reg.get(id).is_none() && !taken.contains(id))
        .unwrap_or(stem)
}

/// Make a prepared show without compositing: seal its outputs, write it
/// down, hand the host the table. All or nothing: a failure takes back
/// what was sealed.
pub fn make_direct(st: &Station, p: Prepared) -> Result<String, RpcError> {
    let records = outputs::seal(&p.id, &[], &p.outputs).inspect_err(|_| outputs::forget(&p.id))?;
    let mut record = Record::new(&p.id, &p.name, None);
    record.compositing = false;
    record.input = p.input;
    record.outputs = records;
    {
        let mut reg = st.registry.lock();
        reg.records.push(record);
        if let Err(e) = reg.save() {
            reg.records.retain(|r| r.id != p.id);
            drop(reg);
            outputs::forget(&p.id);
            return Err(RpcError::internal(format!("saving the list of shows: {e:#}")));
        }
    }
    st.procs.lock().entry(p.id.clone()).or_default();
    st.direct.hand_over();
    st.announce(&p.id);
    Ok(p.id)
}

/// `show.add` of one show without compositing: refused when the planner
/// cannot serve one of its renditions, made otherwise.
pub fn priced_direct(st: &Station, p: Prepared) -> Result<String, RpcError> {
    if let Err(no) = st.direct.price(&p.outputs) {
        return Err(RpcError::invalid_params(no.message.clone()).with("refusal", serde_json::to_value(&no).unwrap_or_default()));
    }
    make_direct(st, p)
}

/// The show a `show.output.*` call names, which must run without
/// compositing.
fn direct_show(st: &Station, id: &str) -> Result<Record, RpcError> {
    let reg = st.registry.lock();
    let r = reg.get(id).cloned().ok_or_else(|| RpcError::not_found("show", id, &reg.ids()))?;
    if r.compositing {
        let msg = format!("show {id} composites, so its outputs are inside it: use output.add with ?show={id}, or turn compositing off with show.set.");
        return Err(RpcError::not_in_state(msg).with("show", id).with("compositing", true));
    }
    Ok(r)
}

/// Run an edit on a direct show's outputs, seal, save and hand over.
pub fn edit_outputs(st: &Arc<Station>, id: &str, edit: impl FnOnce(&mut Vec<StoredDestination>) -> Result<(), RpcError>) -> Result<Value, RpcError> {
    let r = direct_show(st, id)?;
    let before = outputs::stored(id, &r.outputs);
    let mut after = before.clone();
    edit(&mut after)?;
    let records = outputs::seal(id, &before, &after)?;
    {
        let mut reg = st.registry.lock();
        if let Some(r) = reg.get_mut(id) {
            r.outputs = records;
        }
        reg.save().map_err(|e| RpcError::internal(format!("saving the list of shows: {e:#}")))?;
    }
    st.direct.hand_over();
    st.announce(id);
    let show = st.view(id).ok_or_else(|| RpcError::internal(format!("show {id} went while it was being changed")))?;
    Ok(serde_json::to_value(show).unwrap_or_default())
}
