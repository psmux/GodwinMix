//! `preset_save`, `preset_recall`, `preset_list`. The camera stores where a
//! preset points; this stores its name and a thumbnail of the shot, taken
//! from the mixer at the moment it is saved.

use godwinmix_sdk::prelude::{RpcError, ToolResult};
use serde_json::{json, Value};

use super::args::{camera, failed, preset, text, WAIT};
use super::answer;
use crate::rig::Rig;

/// Thumbnails are this wide: enough for a grid on a tablet.
pub const THUMB_WIDTH: u32 = 240;

pub fn save(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    let core = rig.core.clone();
    let cam = camera(rig, a)?;
    let n = preset(a, cam)?;
    let id = cam.spec.id.clone();
    cam.handle.call(WAIT, move |d| d.save_preset(n)).map_err(|e| failed(&id, e))?;
    let name = text(a, "name").map(str::to_string).unwrap_or_else(|| format!("Preset {n}"));
    let (jpeg, why) = match (&core, &cam.spec.source) {
        (Some(core), Some(source)) => match core.snapshot(source, THUMB_WIDTH) {
            Ok(j) => (Some(j), None),
            Err(e) => (None, Some(e)),
        },
        (None, _) => (None, Some("there is no mixer to take a picture from".to_string())),
        (_, None) => (None, Some(format!("'{id}' has no `source`; set one with camera_add for presets with pictures"))),
    };
    let p = cam.presets.save(n, &name, jpeg.as_deref()).map_err(|e| failed(&id, format!("the preset is on the camera, but its name and picture could not be written: {e}")))?;
    let text = match &why {
        None => format!("saved preset {n} '{name}' on '{id}', with a picture."),
        Some(w) => format!("saved preset {n} '{name}' on '{id}', without a picture: {w}."),
    };
    let mut facts = p.public(false);
    facts["camera"] = json!(id);
    facts["thumbnail_problem"] = json!(why);
    Ok(answer(text, facts))
}

pub fn recall(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    let cam = camera(rig, a)?;
    let n = preset(a, cam)?;
    if let Some(f) = &cam.follow {
        f.stop();
    }
    let id = cam.spec.id.clone();
    cam.handle.call(WAIT, move |d| d.goto_preset(n)).map_err(|e| failed(&id, e))?;
    let name = cam.presets.get(n).map(|p| p.name).unwrap_or_else(|| format!("Preset {n}"));
    Ok(answer(format!("'{id}' going to preset {n} '{name}'. Tracking, if it was on, is off."), json!({"camera": id, "preset": n, "name": name})))
}

pub fn list(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    let inline = a.get("pictures").and_then(Value::as_bool).unwrap_or(false);
    let cam = camera(rig, a)?;
    let list: Vec<Value> = cam.presets.list().iter().map(|p| p.public(inline)).collect();
    let text = format!("'{}' has {} saved preset(s) with names.", cam.spec.id, list.len());
    Ok(answer(text, json!({"camera": cam.spec.id, "presets": list})))
}
