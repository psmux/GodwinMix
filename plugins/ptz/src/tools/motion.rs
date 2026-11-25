//! `move` (the joystick), `stop`, `zoom`, `focus`, `exposure`, `position`,
//! `goto`. `move` answers at once; the others wait on the camera off the
//! rig's lock, and `stop` and `zoom` go on the camera's motion lane so they
//! never queue behind a slow command.

use godwinmix_ptz::{Exposure, ExposureMode, Focus, Position, Velocity};
use godwinmix_sdk::prelude::{RpcError, ToolResult};
use serde_json::{json, Value};

use super::args::{camera, failed, invalid, ranged, text, view, WAIT};
use super::{answer, later, Plan, Work};
use crate::rig::Rig;

/// The joystick: a velocity that lasts the hold time (300 ms) unless renewed.
/// A panel calls it every 100 ms or so while the stick is held. Taking the
/// joystick also hands the camera back from the follow loop.
pub fn joystick(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    let v = Velocity::new(ranged(a, "pan", -1.0, 1.0)?.unwrap_or(0.0), ranged(a, "tilt", -1.0, 1.0)?.unwrap_or(0.0), ranged(a, "zoom", -1.0, 1.0)?.unwrap_or(0.0));
    let hold = rig.settings.joystick_hold.as_millis() as u64;
    let cam = camera(rig, a)?;
    cam.handle.joystick(v);
    let last_error = cam.handle.status.lock().unwrap().last_error.clone();
    let text = match &last_error {
        Some(e) => format!("sent, but the last command to '{}' failed: {e}", cam.spec.id),
        None if v.is_stop() => format!("'{}' stopped.", cam.spec.id),
        None => format!("'{}' moving for {hold} ms unless moved again.", cam.spec.id),
    };
    Ok(answer(text, json!({"camera": cam.spec.id, "pan": v.pan, "tilt": v.tilt, "zoom": v.zoom, "hold_ms": hold, "last_error": last_error})))
}

pub fn stop(rig: &mut Rig, a: &Value) -> Plan {
    later(camera(rig, a).map(|cam| {
        if let Some(f) = &cam.follow {
            f.stop();
        }
        let cam = cam.view();
        Box::new(move |_: &_| {
            let id = cam.spec.id.clone();
            cam.handle.halt(WAIT).map_err(|e| failed(&id, e))?;
            Ok(answer(format!("'{id}' stopped, and tracking is off."), json!({"camera": id})))
        }) as Work
    }))
}

pub fn zoom(rig: &mut Rig, a: &Value) -> Plan {
    let mut take = || -> Result<Work, RpcError> {
        let speed = ranged(a, "speed", -1.0, 1.0)?.ok_or_else(|| invalid("zoom wants `speed`, -1 (wider) to 1 (tighter), 0 to stop. It keeps zooming until a zoom with speed 0, or use move for a zoom that stops by itself."))?;
        let cam = view(rig, a)?;
        Ok(Box::new(move |_: &_| {
            let id = cam.spec.id.clone();
            cam.handle.motion_call(WAIT, move |d| d.zoom(speed)).map_err(|e| failed(&id, e))?;
            Ok(answer(format!("'{id}' zoom at {speed}."), json!({"camera": id, "speed": speed})))
        }))
    };
    later(take())
}

pub fn focus(rig: &mut Rig, a: &Value) -> Plan {
    let mut take = || -> Result<Work, RpcError> {
        let speed = ranged(a, "speed", 0.0, 1.0)?.unwrap_or(0.5);
        let mode = text(a, "mode").unwrap_or("").to_string();
        let f = match mode.as_str() {
            "auto" => Focus::Auto,
            "manual" => Focus::Manual,
            "one_push" => Focus::OnePush,
            "near" => Focus::Drive(-speed),
            "far" => Focus::Drive(speed),
            "stop" => Focus::Drive(0.0),
            other => return Err(invalid(format!("focus mode '{other}' is not one of auto, manual, one_push, near, far, stop."))),
        };
        let cam = view(rig, a)?;
        Ok(Box::new(move |_: &_| {
            let id = cam.spec.id.clone();
            cam.handle.call(WAIT, move |d| d.focus(f)).map_err(|e| failed(&id, e))?;
            Ok(answer(format!("'{id}' focus {mode}."), json!({"camera": id, "mode": mode})))
        }))
    };
    later(take())
}

pub fn exposure(rig: &mut Rig, a: &Value) -> Plan {
    let mut take = || -> Result<Work, RpcError> {
        let mode = match text(a, "mode") {
            Some(m) => Some(ExposureMode::parse(m).ok_or_else(|| invalid(format!("exposure mode '{m}' is not one of auto, manual, shutter, iris.")))?),
            None => None,
        };
        let step = |k: &str| a.get(k).and_then(Value::as_u64).map(|v| v.min(u16::MAX as u64) as u16);
        let e = Exposure { mode, iris: step("iris"), shutter: step("shutter"), gain: step("gain") };
        let (cam, mode) = (view(rig, a)?, text(a, "mode").map(str::to_string));
        Ok(Box::new(move |_: &_| {
            let id = cam.spec.id.clone();
            cam.handle.call(WAIT, move |d| d.exposure(e)).map_err(|e| failed(&id, e))?;
            Ok(answer(format!("'{id}' exposure set."), json!({"camera": id, "mode": mode, "iris": e.iris, "shutter": e.shutter, "gain": e.gain})))
        }))
    };
    later(take())
}

pub fn position(rig: &mut Rig, a: &Value) -> Plan {
    later(view(rig, a).map(|cam| {
        Box::new(move |_: &_| {
            let id = cam.spec.id.clone();
            let p = cam.handle.call(WAIT, |d| d.position()).map_err(|e| failed(&id, e))?;
            let text = format!("'{id}' is at pan {:.2}, tilt {:.2}, zoom {:.2}.", p.pan, p.tilt, p.zoom);
            Ok(answer(text, json!({"camera": id, "pan": p.pan, "tilt": p.tilt, "zoom": p.zoom, "raw": p.raw})))
        }) as Work
    }))
}

pub fn goto(rig: &mut Rig, a: &Value) -> Plan {
    let mut take = || -> Result<Work, RpcError> {
        let (pan, tilt, zoom) = (ranged(a, "pan", -1.0, 1.0)?, ranged(a, "tilt", -1.0, 1.0)?, ranged(a, "zoom", 0.0, 1.0)?);
        let cam = view(rig, a)?;
        Ok(Box::new(move |_: &_| {
            let id = cam.spec.id.clone();
            let now = cam.handle.call(WAIT, |d| d.position()).unwrap_or_default();
            let p = Position { pan: pan.unwrap_or(now.pan), tilt: tilt.unwrap_or(now.tilt), zoom: zoom.unwrap_or(now.zoom), raw: [0; 3] };
            cam.handle.call(WAIT, move |d| d.goto(p)).map_err(|e| failed(&id, e))?;
            Ok(answer(format!("'{id}' going to pan {:.2}, tilt {:.2}, zoom {:.2}.", p.pan, p.tilt, p.zoom), json!({"camera": id, "pan": p.pan, "tilt": p.tilt, "zoom": p.zoom})))
        }))
    };
    later(take())
}
