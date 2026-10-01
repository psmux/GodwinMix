//! `show.set` and `show.output.*`.

use super::shows_direct::{check_input, edit_outputs};
use super::state::Station;
use super::{direct, switch};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::*;
use serde_json::Value;
use std::sync::Arc;

fn parse<T: serde::de::DeserializeOwned>(method: &str, params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|e| RpcError::invalid_params(format!("{method}: {e}")))
}

fn saved(st: &Station) -> Result<(), RpcError> {
    st.registry.lock().save().map_err(|e| RpcError::internal(format!("saving the list of shows: {e:#}")))
}

/// `show.set`: the name and the input first, then the switch, so a show
/// can be given an input and turned to direct in one call. A switch answers
/// with a task handle rather than the show.
pub async fn set(st: &Arc<Station>, req: ShowSetRequest) -> Result<Value, RpcError> {
    let (was, has_input) = {
        let reg = st.registry.lock();
        let r = reg.get(&req.id).ok_or_else(|| RpcError::not_found("show", &req.id, &reg.ids()))?;
        (r.compositing, r.input.is_some())
    };
    let name = match &req.name {
        Some(n) if n.trim().is_empty() => return Err(RpcError::invalid_params("a show needs a name, such as \"Second room\"").with("field", "name")),
        Some(n) => Some(n.trim().to_string()),
        None => None,
    };
    if let Some(input) = &req.input {
        check_input(input)?;
    }
    if req.compositing == Some(false) && !has_input && req.input.is_none() {
        let msg = format!("show {} has no input, and a show without compositing sends one input on. Send `input` in the same call.", req.id);
        return Err(RpcError::invalid_params(msg).with("field", "input").with("show", req.id.clone()));
    }
    if name.is_some() || req.input.is_some() || req.alarms.is_some() {
        // Sealed before the list is locked: the secret store writes a file.
        let input = direct::inputs::seal_some(&req.id, req.input.clone())?;
        if let Some(r) = st.registry.lock().get_mut(&req.id) {
            if let Some(n) = name {
                r.name = n;
            }
            if input.is_some() {
                r.input = input;
            }
            if let Some(a) = &req.alarms {
                r.alarms = Some(r.alarms.clone().unwrap_or_default().merged(a));
            }
        }
        saved(st)?;
        if req.input.is_some() || req.alarms.is_some() {
            st.direct.hand_over();
        }
        // A show that composites judges its own programme. One that is not
        // running yet is handed its alarms when it says hello.
        if req.alarms.is_some() && was && st.state_of(&req.id) == Some(ShowState::Running) {
            direct::hand_alarms(st, &req.id).await;
        }
    }
    // A switch takes up to half a minute, so it runs as a task and this
    // answers with its handle; see `switch::task`.
    match req.compositing {
        Some(true) if !was => return switch::task::start(st, &req.id, true, Vec::new()),
        Some(false) if was => {
            let moving = switch::check_off(st, &req.id).await?;
            return switch::task::start(st, &req.id, false, moving);
        }
        _ => {}
    }
    st.announce(&req.id);
    let show = st.view(&req.id).ok_or_else(|| RpcError::internal(format!("show {} went while it was being changed", req.id)))?;
    Ok(serde_json::to_value(ShowSetResult { show, switch: None }).unwrap_or_default())
}

/// `show.output.add`, `.set` and `.remove`, on a show without compositing.
pub fn output(st: &Arc<Station>, method: &str, params: Value) -> Result<Value, RpcError> {
    match method {
        "show.output.add" => {
            let req: ShowOutputAddRequest = parse(method, params)?;
            // Priced before it is kept: an output the planner cannot serve
            // is refused here rather than written down and left failing.
            edit_outputs(st, &req.id, |list| {
                let made = direct::add_output(list, &req.id, &req.spec())?;
                direct::hls::check_sound(st, &req.id, list.iter().find(|d| d.id == made))?;
                st.direct
                    .price(list)
                    .map(|_| ())
                    .map_err(|no| RpcError::invalid_params(no.message.clone()).with("refusal", serde_json::to_value(&no).unwrap_or_default()))
            })
        }
        "show.output.set" => {
            let req: ShowOutputSetRequest = parse(method, params)?;
            edit_outputs(st, &req.id, |list| direct::set_output(list, &req))
        }
        "show.output.remove" => {
            let req: ShowOutputRemoveRequest = parse(method, params)?;
            edit_outputs(st, &req.id, |list| {
                let ids: Vec<String> = list.iter().map(|d| d.id.clone()).collect();
                let at = list.iter().position(|d| d.id == req.output).ok_or_else(|| RpcError::not_found("output", &req.output, &ids).with("show", req.id.clone()))?;
                list.remove(at);
                Ok(())
            })
        }
        other => Err(RpcError::not_found("method", other, &[])),
    }
}
