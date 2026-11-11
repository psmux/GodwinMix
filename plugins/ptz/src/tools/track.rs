//! `track_follow`, `track_stop`, `track_framing`, `track_state` and
//! `track_observe`: the plan's `track/follow`, `track/stop`, `track/state`
//! and `track/observe`, spelled with underscores because tool names are.

use std::time::Duration;

use godwinmix_sdk::prelude::{codes, RpcError, ToolResult};
use godwinmix_track::{Choose, Framing, Point, TrackId};
use serde_json::{json, Value};

use super::args::{camera, invalid, num, text};
use super::{answer, later, Plan, Work};
use crate::follow::detector::Switching;
use crate::follow::{view, wait_seen, Ctl, Look, Loop};
use crate::rig::{Camera, Rig};

/// An observing loop ends this long after the last call that asked.
pub const LINGER: Duration = Duration::from_secs(15);

/// The camera's loop, started when there is none (or the last one ended).
fn ensure<'a>(rig: &'a mut Rig, a: &Value, steering: bool) -> Result<&'a Camera, RpcError> {
    let (core, width, framing) = (rig.core.clone(), rig.settings.track_width, rig.settings.framing);
    let (kind, models) = (rig.settings.detector, rig.settings.models_dir.clone());
    let cam = camera(rig, a)?;
    if !cam.follow.as_ref().is_some_and(Loop::is_running) {
        let frames = crate::frames::open(core.as_ref(), &cam.spec, width).map_err(|why| RpcError::new(codes::WRONG_STATE, why).with_data(json!({"camera": cam.spec.id})))?;
        let detector = Switching::new(kind, models);
        let look = Look { framing, steering, optics: Some(cam.spec.optics) };
        cam.follow = Some(Loop::start(cam.handle.clone(), frames, detector, look, LINGER));
    } else if steering {
        cam.follow.as_ref().expect("checked").send(Ctl::Steer(true));
    }
    cam.follow.as_ref().expect("just made").touch();
    Ok(cam)
}

fn framing_arg(a: &Value) -> Result<Option<Framing>, RpcError> {
    match text(a, "framing") {
        Some(f) => Framing::from_slug(f).map(Some).ok_or_else(|| invalid(format!("framing '{f}' is not one of close, waist, full, wide."))),
        None => Ok(None),
    }
}

/// Who to follow, from `target` ("person-3", "largest", "central") or
/// `point` ({x, y}, 0 to 1 from the top left, where a person tapped).
fn who(a: &Value) -> Result<Option<Ctl>, RpcError> {
    if let Some(p) = a.get("point").filter(|p| !p.is_null()) {
        let (x, y) = (num(p, "x"), num(p, "y"));
        return match (x, y) {
            (Some(x), Some(y)) if (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) => Ok(Some(Ctl::Point(Point::new(x, y)))),
            _ => Err(invalid("`point` wants x and y from 0 to 1, from the top left of the picture: {x: 0.4, y: 0.3}.")),
        };
    }
    Ok(match text(a, "target") {
        None => None,
        Some("largest") => Some(Ctl::Policy(Choose::Largest)),
        Some("central") => Some(Ctl::Policy(Choose::Central)),
        Some(id) => Some(Ctl::Lock(id.parse::<TrackId>().map_err(|_| invalid(format!("target '{id}' is not a person id such as person-3, nor largest or central. track_observe lists the ids in view.")))?)),
    })
}

pub fn follow(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    let (framing, target) = (framing_arg(a)?, who(a)?);
    let cam = ensure(rig, a, true)?;
    let l = cam.follow.as_ref().expect("ensured");
    if let Some(f) = framing {
        l.send(Ctl::Framing(f));
    }
    if let Some(t) = target {
        l.send(t);
    }
    let s = l.seen.lock().unwrap().clone();
    let style = framing.or(s.framing).map(|f| f.slug()).unwrap_or("waist");
    Ok(answer(format!("'{}' is tracking, {style} shot. track_state says who it follows.", cam.spec.id), view::seen(&cam.spec.id, &s)))
}

pub fn stop(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    let cam = camera(rig, a)?;
    if let Some(l) = cam.follow.take() {
        l.stop();
    }
    Ok(answer(format!("'{}' tracking is off; the camera stays where it is.", cam.spec.id), json!({"camera": cam.spec.id, "tracking": false})))
}

pub fn framing(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    let f = framing_arg(a)?.ok_or_else(|| invalid("track_framing wants `framing`: close, waist, full or wide."))?;
    rig.settings.framing = f;
    let cam = camera(rig, a)?;
    if let Some(l) = cam.follow.as_ref().filter(|l| l.is_running()) {
        l.send(Ctl::Framing(f));
    }
    Ok(answer(format!("'{}' framing is {}.", cam.spec.id, f.slug()), json!({"camera": cam.spec.id, "framing": f.slug()})))
}

pub fn state(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    if a.get("camera").is_none() && rig.cameras.len() != 1 {
        let all: Vec<Value> = rig.cameras.iter().map(|(id, c)| summary(id, c)).collect();
        return Ok(answer(format!("{} camera(s).", all.len()), json!({"cameras": all})));
    }
    let cam = camera(rig, a)?;
    let v = summary(&cam.spec.id, cam);
    let text = match &cam.follow {
        Some(l) if l.is_running() => view::sentence(&cam.spec.id, &l.seen.lock().unwrap()),
        _ => format!("{}: tracking is off.", cam.spec.id),
    };
    Ok(answer(text, v))
}

fn summary(id: &str, c: &Camera) -> Value {
    match &c.follow {
        Some(l) if l.is_running() => {
            l.touch();
            view::seen(id, &l.seen.lock().unwrap())
        }
        _ => json!({"camera": id, "mode": "stopped", "tracks": []}),
    }
}

/// What the camera sees now. Starts an observing loop if none runs and
/// waits for a few pictures, off the rig's lock, so the first answer is not
/// empty and no other tool waits for it.
pub fn observe(rig: &mut Rig, a: &Value) -> Plan {
    let wait = Duration::from_millis(a.get("wait_ms").and_then(Value::as_u64).unwrap_or(1_500).min(5_000));
    let take = |rig: &mut Rig| -> Result<Work, RpcError> {
        let cam = ensure(rig, a, false)?;
        let (id, seen) = (cam.spec.id.clone(), cam.follow.as_ref().expect("ensured").seen.clone());
        Ok(Box::new(move |_: &_| {
            // A loop already running answers at once; a new one gets a
            // moment to see a few pictures, since motion needs more than one.
            if seen.lock().unwrap().frames == 0 {
                wait_seen(&seen, 5, wait);
            }
            let s = seen.lock().unwrap().clone();
            Ok(answer(view::sentence(&id, &s), view::seen(&id, &s)))
        }))
    };
    later(take(rig))
}
