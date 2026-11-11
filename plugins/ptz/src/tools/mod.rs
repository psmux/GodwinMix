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

pub mod args;
pub mod camera;
pub mod motion;
pub mod preset;
pub mod track;

use godwinmix_sdk::prelude::{codes, RpcError, ToolResult};
use serde_json::{json, Value};

use crate::rig::Rig;

pub const NAMES: [&str; 18] = [
    "camera_add", "camera_list", "camera_remove", "move", "stop", "zoom", "focus", "exposure", "position", "goto", "preset_save", "preset_recall", "preset_list",
    "track_follow", "track_stop", "track_framing", "track_state", "track_observe",
];

pub fn call(rig: &mut Rig, name: &str, a: &Value) -> Result<ToolResult, RpcError> {
    match name {
        "camera_add" => camera::add(rig, a),
        "camera_list" => camera::list(rig),
        "camera_remove" => camera::remove(rig, a),
        "move" => motion::joystick(rig, a),
        "stop" => motion::stop(rig, a),
        "zoom" => motion::zoom(rig, a),
        "focus" => motion::focus(rig, a),
        "exposure" => motion::exposure(rig, a),
        "position" => motion::position(rig, a),
        "goto" => motion::goto(rig, a),
        "preset_save" => preset::save(rig, a),
        "preset_recall" => preset::recall(rig, a),
        "preset_list" => preset::list(rig, a),
        "track_follow" => track::follow(rig, a),
        "track_stop" => track::stop(rig, a),
        "track_framing" => track::framing(rig, a),
        "track_state" => track::state(rig, a),
        "track_observe" => track::observe(rig, a),
        other => Err(RpcError::new(codes::METHOD_NOT_FOUND, format!("gmx-ptz has no tool '{other}'. It has: {}.", NAMES.join(", ")))),
    }
}

/// A sentence and the facts.
pub fn answer(text: impl Into<String>, facts: Value) -> ToolResult {
    ToolResult { content: json!([{"type": "text", "text": text.into()}]), structured_content: Some(facts), is_error: None }
}
