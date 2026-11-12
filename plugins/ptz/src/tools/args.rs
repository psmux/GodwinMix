//! Reading tool arguments, and the refusals every tool shares.

use std::time::Duration;

use godwinmix_sdk::prelude::{codes, RpcError};
use serde_json::{json, Value};

use crate::rig::{Camera, Rig, View};

/// How long a tool waits for a camera to answer one command.
pub const WAIT: Duration = Duration::from_secs(5);

pub fn invalid(message: impl Into<String>) -> RpcError {
    RpcError::new(codes::INVALID_PARAMS, message)
}

/// The camera named by `camera`, or a refusal listing the ones there are.
pub fn camera<'a>(rig: &'a mut Rig, a: &Value) -> Result<&'a mut Camera, RpcError> {
    let ids: Vec<String> = rig.ids().iter().map(|s| s.to_string()).collect();
    let id = match a.get("camera").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None if ids.len() == 1 => ids[0].clone(),
        None => return Err(invalid(format!("say which camera with `camera`. The cameras are: {}.", list(&ids))).with_data(json!({"cameras": ids}))),
    };
    let known = ids.clone();
    rig.cameras.get_mut(&id).ok_or_else(|| {
        RpcError::new(codes::NOT_FOUND, format!("there is no camera '{id}'. The cameras are: {}. Add one with camera_add.", list(&known))).with_data(json!({"cameras": known}))
    })
}

/// The camera named by `camera`, to wait on without the rig's lock.
pub fn view(rig: &mut Rig, a: &Value) -> Result<View, RpcError> {
    camera(rig, a).map(|c| c.view())
}

fn list(ids: &[String]) -> String {
    if ids.is_empty() {
        "none yet".into()
    } else {
        ids.join(", ")
    }
}

pub fn num(a: &Value, key: &str) -> Option<f32> {
    a.get(key).and_then(Value::as_f64).map(|v| v as f32)
}

pub fn text<'a>(a: &'a Value, key: &str) -> Option<&'a str> {
    a.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// A speed or position that must be in `lo..=hi`.
pub fn ranged(a: &Value, key: &str, lo: f32, hi: f32) -> Result<Option<f32>, RpcError> {
    match num(a, key) {
        Some(v) if (lo..=hi).contains(&v) => Ok(Some(v)),
        Some(v) => Err(invalid(format!("`{key}` is {v}; it goes from {lo} to {hi}."))),
        None if a.get(key).is_some_and(|v| !v.is_null()) => Err(invalid(format!("`{key}` must be a number from {lo} to {hi}."))),
        None => Ok(None),
    }
}

/// A preset number, 1 and up.
pub fn preset(a: &Value, cam: &View) -> Result<u16, RpcError> {
    let n = a.get("preset").and_then(Value::as_u64).ok_or_else(|| invalid(format!("say which preset with `preset`, a number from 1 to {}.", cam.caps.presets)))?;
    if n == 0 || n > u64::from(cam.caps.presets) {
        return Err(invalid(format!("preset {n} is out of range for '{}'. Its presets are 1 to {}.", cam.spec.id, cam.caps.presets)));
    }
    Ok(n as u16)
}

/// A camera that refused or did not answer.
pub fn failed(camera: &str, why: String) -> RpcError {
    RpcError::new(codes::WRONG_STATE, format!("camera '{camera}': {why}")).with_data(json!({"camera": camera}))
}
