//! The tools agents and the camera window call. Every one answers a sentence
//! for a person in `content` and the facts in `structured_content`, and every
//! refusal says what is wrong and what to do.
//!
//! | Module | Tools |
//! |---|---|
//! | [`camera`] | `camera_add`, `camera_list`, `camera_remove` |
//! | [`motion`] | `move`, `stop`, `zoom`, `focus`, `exposure`, `position`, `goto` |
//! | [`preset`] | `preset_save`, `preset_recall`, `preset_list` |
//! | [`track`] | `track_follow`, `track_stop`, `track_framing`, `track_state`, `track_observe` |
//!
//! Tools that only look at the rig (`move`, `camera_list`, `track_state`)
//! answer at once under its lock. Tools that wait on a camera or the network
//! take what they need under the lock, let it go, and do the waiting on a
//! thread of their own ([`Plan::Later`]), so a slow `preset_save` on one
//! camera never holds up a `move` or `stop` on it or on any other.

pub mod args;
pub mod camera;
pub mod motion;
pub mod preset;
pub mod track;
pub mod tune;

use std::sync::{Arc, Mutex};

use godwinmix_sdk::prelude::{codes, RpcError, ToolResult};
use serde_json::{json, Value};

use crate::rig::Rig;

pub const NAMES: [&str; 19] = [
    "camera_add", "camera_list", "camera_remove", "move", "stop", "zoom", "focus", "exposure", "position", "goto", "preset_save", "preset_recall", "preset_list",
    "track_follow", "track_stop", "track_framing", "track_state", "track_observe", "track_tune",
];

/// The rig, shared by the tool calls in flight.
pub type Shared = Arc<Mutex<Rig>>;

pub type Outcome = Result<ToolResult, RpcError>;
pub type Work = Box<dyn FnOnce(&Shared) -> Outcome + Send>;

/// How a tool answers: now, under the rig's lock, or later, off it.
pub enum Plan {
    Now(Outcome),
    Later(Work),
}

impl From<Outcome> for Plan {
    fn from(o: Outcome) -> Plan {
        Plan::Now(o)
    }
}

/// Later, unless taking what the work needs already failed.
pub fn later(work: Result<Work, RpcError>) -> Plan {
    match work {
        Ok(w) => Plan::Later(w),
        Err(e) => Plan::Now(Err(e)),
    }
}

pub fn plan(rig: &mut Rig, name: &str, a: &Value) -> Plan {
    match name {
        "camera_add" => camera::add(rig, a),
        "camera_list" => camera::list(rig).into(),
        "camera_remove" => camera::remove(rig, a).into(),
        "move" => motion::joystick(rig, a).into(),
        "stop" => motion::stop(rig, a),
        "zoom" => motion::zoom(rig, a),
        "focus" => motion::focus(rig, a),
        "exposure" => motion::exposure(rig, a),
        "position" => motion::position(rig, a),
        "goto" => motion::goto(rig, a),
        "preset_save" => preset::save(rig, a),
        "preset_recall" => preset::recall(rig, a),
        "preset_list" => preset::list(rig, a).into(),
        "track_follow" => track::follow(rig, a).into(),
        "track_stop" => track::stop(rig, a).into(),
        "track_framing" => track::framing(rig, a).into(),
        "track_state" => track::state(rig, a).into(),
        "track_observe" => track::observe(rig, a),
        "track_tune" => tune::tune(rig, a).into(),
        other => Plan::Now(Err(RpcError::new(codes::METHOD_NOT_FOUND, format!("gmx-ptz has no tool '{other}'. It has: {}.", NAMES.join(", "))))),
    }
}

/// Call a tool and wait for its answer, on this thread.
pub fn call(rig: &Shared, name: &str, a: &Value) -> Outcome {
    let p = plan(&mut rig.lock().unwrap_or_else(|e| e.into_inner()), name, a);
    match p {
        Plan::Now(o) => o,
        Plan::Later(w) => w(rig),
    }
}

/// Call a tool and hand its answer to `done`: at once for a tool that only
/// looks at the rig, from a thread of its own for one that waits.
pub fn call_later(rig: &Shared, name: &str, a: &Value, done: impl FnOnce(Outcome) + Send + 'static) {
    let p = plan(&mut rig.lock().unwrap_or_else(|e| e.into_inner()), name, a);
    match p {
        Plan::Now(o) => done(o),
        Plan::Later(w) => {
            let rig = rig.clone();
            let spawned = std::thread::Builder::new().name(format!("ptz-tool-{name}")).spawn(move || done(w(&rig)));
            if let Err(e) = spawned {
                eprintln!("gmx-ptz: could not start a thread for '{name}': {e}");
            }
        }
    }
}

/// A sentence and the facts.
pub fn answer(text: impl Into<String>, facts: Value) -> ToolResult {
    ToolResult { content: json!([{"type": "text", "text": text.into()}]), structured_content: Some(facts), is_error: None }
}
