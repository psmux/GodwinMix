//! `camera_add`, `camera_list`, `camera_remove`.

use godwinmix_sdk::prelude::{codes, RpcError, ToolResult};
use serde_json::{json, Value};

use super::args::{camera, invalid};
use super::answer;
use crate::rig::{Camera, Rig};
use crate::settings::CameraSpec;

pub fn add(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    let spec = CameraSpec::from_value(a).map_err(invalid)?;
    if rig.cameras.get(&spec.id).is_some_and(|c| !c.added) {
        return Err(invalid(format!("'{}' comes from the plugin's settings. Change it there, or pick another id.", spec.id)));
    }
    let replaced = rig.remove(&spec.id);
    let id = spec.id.clone();
    let cam = rig.add(spec, true).map_err(|why| RpcError::new(codes::WRONG_STATE, why).with_data(json!({"camera": id})))?;
    let verb = if replaced { "replaced" } else { "added" };
    let picture = if cam.spec.source.is_none() && cam.spec.bus.is_none() { " It has no `source`, so presets have no pictures and tracking is off until one is set." } else { "" };
    Ok(answer(format!("{verb} camera '{}' ({}, {}).{picture}", cam.spec.id, cam.spec.protocol.slug(), cam.spec.address), record(cam)))
}

/// One camera as the list shows it.
pub fn record(c: &Camera) -> Value {
    let s = c.handle.status.lock().unwrap().clone();
    let mut v = c.spec.public();
    v["capabilities"] = json!({"absolute": c.caps.absolute, "position": c.caps.position, "focus": c.caps.focus, "exposure": c.caps.exposure, "presets": c.caps.presets});
    v["moving"] = json!({"pan": s.moving.pan, "tilt": s.moving.tilt, "zoom": s.moving.zoom, "by_joystick": s.by_joystick});
    v["last_error"] = json!(s.last_error);
    v["tracking"] = json!(c.follow.as_ref().is_some_and(|f| f.is_running() && f.is_steering()));
    v["added_by_tool"] = json!(c.added);
    v
}

pub fn list(rig: &mut Rig) -> Result<ToolResult, RpcError> {
    let cams: Vec<Value> = rig.cameras.values().map(record).collect();
    let text = if cams.is_empty() {
        "no cameras yet. Add one with camera_add {id, protocol, address}.".to_string()
    } else {
        format!("{} camera(s): {}.", cams.len(), rig.ids().join(", "))
    };
    Ok(answer(text, json!({"cameras": cams, "problems": rig.problems})))
}

pub fn remove(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    let id = camera(rig, a)?.spec.id.clone();
    if !rig.cameras[&id].added {
        return Err(invalid(format!("'{id}' comes from the plugin's settings; remove it there.")));
    }
    rig.remove(&id);
    Ok(answer(format!("removed camera '{id}'. It was stopped on the way out."), json!({"removed": id})))
}
