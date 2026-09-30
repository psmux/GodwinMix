//! `show.add`, `show.rename`, `show.remove`, `show.start` and `show.stop`.

use super::registry::{Record, MAIN};
use super::state::Station;
use super::{files, supervise};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::{Show, ShowAddRequest, ShowFrom, ShowRemoved, ShowRenameRequest};
use godwinmix_protocol::types::Event;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// How long `show.add {from: {project}}` waits for the new show to come up.
const PROJECT_WAIT: Duration = Duration::from_secs(30);

fn saved(st: &Station) -> Result<(), RpcError> {
    st.registry.lock().save().map_err(|e| RpcError::internal(format!("saving the list of shows: {e:#}")))
}

fn named(name: &str) -> Result<String, RpcError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(RpcError::invalid_params("a show needs a name, such as \"Second room\"").with("field", "name"));
    }
    Ok(name.to_string())
}

fn known(st: &Station, id: &str) -> Result<(), RpcError> {
    let reg = st.registry.lock();
    match reg.get(id) {
        Some(_) => Ok(()),
        None => Err(RpcError::not_found("show", id, &reg.ids())),
    }
}

fn shown(st: &Station, id: &str) -> Result<Value, RpcError> {
    let show: Show = st.view(id).ok_or_else(|| RpcError::internal(format!("show {id} went while it was being changed")))?;
    Ok(serde_json::to_value(show).unwrap_or_default())
}

pub async fn add(st: &Arc<Station>, req: ShowAddRequest) -> Result<Value, RpcError> {
    let name = named(&req.name)?;
    let project = match &req.from {
        Some(ShowFrom::Project { project }) => Some(super::project::usable(project)?),
        _ => None,
    };
    let (id, folder, from) = {
        let reg = st.registry.lock();
        let from = match &req.from {
            Some(ShowFrom::Named(n)) if n != "empty" => {
                let record = reg.get(n).ok_or_else(|| RpcError::not_found("show to copy", n, &reg.ids()))?;
                Some(reg.config_of(record))
            }
            _ => None,
        };
        let id = reg.free_id(&name);
        (id.clone(), reg.folder_for(&id), from)
    };
    let made = match &from {
        Some(config) => files::copy(config, &folder),
        None => files::fresh(&folder),
    };
    let config = made.map_err(|e| RpcError::internal(format!("making the show's folder {}: {e:#}", folder.display())))?;
    st.registry.lock().records.push(Record { id: id.clone(), name, config: Some(config), stopped: false });
    saved(st)?;
    supervise::start(st, &id);
    if let Some(project) = project {
        super::project::open(st, &id, project, PROJECT_WAIT).await?;
    }
    shown(st, &id)
}

pub fn rename(st: &Arc<Station>, req: ShowRenameRequest) -> Result<Value, RpcError> {
    let name = named(&req.name)?;
    known(st, &req.id)?;
    if let Some(r) = st.registry.lock().get_mut(&req.id) {
        r.name = name;
    }
    saved(st)?;
    st.announce(&req.id);
    shown(st, &req.id)
}

pub async fn remove(st: &Arc<Station>, id: &str) -> Result<Value, RpcError> {
    known(st, id)?;
    if st.registry.lock().records.len() == 1 {
        return Err(RpcError::not_in_state(format!("show {id} is the only show, and a station runs at least one. Add another first.")).with("show", id));
    }
    if id == MAIN {
        return Err(RpcError::not_in_state(
            "main is the show the station was started with, and its config is the station's own. Stop it with show.stop instead.",
        )
        .with("show", id));
    }
    supervise::stop(st, id).await;
    let folder = {
        let mut reg = st.registry.lock();
        let folder = reg.folder_for(id);
        reg.records.retain(|r| r.id != id);
        folder
    };
    st.procs.lock().remove(id);
    saved(st)?;
    if folder.starts_with(st.registry.lock().data_dir().join("shows")) {
        let _ = std::fs::remove_dir_all(&folder);
    }
    st.events.emit(Event::ShowRemoved { id: id.to_string() });
    Ok(serde_json::to_value(ShowRemoved { removed: id.to_string() }).unwrap_or_default())
}

pub async fn start(st: &Arc<Station>, id: &str) -> Result<Value, RpcError> {
    known(st, id)?;
    if let Some(r) = st.registry.lock().get_mut(id) {
        r.stopped = false;
    }
    saved(st)?;
    supervise::start(st, id);
    shown(st, id)
}

pub async fn stop(st: &Arc<Station>, id: &str) -> Result<Value, RpcError> {
    known(st, id)?;
    if let Some(r) = st.registry.lock().get_mut(id) {
        r.stopped = true;
    }
    saved(st)?;
    supervise::stop(st, id).await;
    shown(st, id)
}

fn parse<T: serde::de::DeserializeOwned>(method: &str, params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|e| RpcError::invalid_params(format!("{method}: {e}")))
}

/// The station's answer to `show.*`, by name.
pub async fn call(st: &Arc<Station>, method: &str, params: Value) -> Result<Value, RpcError> {
    let id = || params.get("id").and_then(Value::as_str).map(str::to_string).ok_or_else(|| RpcError::invalid_params("name the show with id").with("field", "id"));
    match method {
        "show.list" => Ok(serde_json::to_value(st.list().await).unwrap_or_default()),
        "show.add" => add(st, parse(method, params.clone())?).await,
        "show.rename" => rename(st, parse(method, params.clone())?),
        "show.remove" => remove(st, &id()?).await,
        "show.start" => start(st, &id()?).await,
        "show.stop" => stop(st, &id()?).await,
        other => Err(RpcError::not_found("method", other, &[]).with("hint", json!("show.list, show.add, show.rename, show.remove, show.start, show.stop"))),
    }
}
