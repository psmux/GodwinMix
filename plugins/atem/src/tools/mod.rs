//! The tools agents and the camera window call, as `atem/<tool>` through the
//! core's `tool.call`. Every one answers a sentence for a person in
//! `content` and the facts in `structured_content`, and every refusal says
//! what is wrong and what to do.
//!
//! | Module | Tools |
//! |---|---|
//! | [`setup`] | `list`, `state`, `tally`, `connect`, `disconnect`, `map`, `mode` |
//! | [`control`] | `cut`, `auto`, `ftb`, `preview`, `program`, `transition` |
//! | [`keys`] | `dsk`, `usk`, `aux`, `macro` |
//!
//! Commands are queued for the switcher's own task and answered at once;
//! what the switcher did shows in `state` a few milliseconds later.

pub mod args;
pub mod control;
pub mod keys;
pub mod setup;

use std::sync::{Arc, Mutex};

use godwinmix_sdk::prelude::{codes, RpcError, ToolResult};
use serde_json::{json, Value};

use crate::desk::Desk;

pub const NAMES: [&str; 17] = [
    "list", "state", "tally", "connect", "disconnect", "map", "mode", "cut", "auto", "preview", "program", "transition", "dsk", "usk", "ftb", "aux", "macro",
];

/// The desk, shared by the tool calls and the follow and lead loop.
pub type Shared = Arc<Mutex<Desk>>;

pub type Outcome = Result<ToolResult, RpcError>;

pub fn call(desk: &Shared, name: &str, a: &Value) -> Outcome {
    let mut desk = desk.lock().unwrap_or_else(|e| e.into_inner());
    match name {
        "list" => setup::list(&desk),
        "state" => setup::state(&desk, a),
        "tally" => setup::tally(&desk, a),
        "connect" => setup::connect(&mut desk, a),
        "disconnect" => setup::disconnect(&mut desk, a),
        "map" => setup::map(&mut desk, a),
        "mode" => setup::mode(&mut desk, a),
        "cut" | "auto" | "ftb" => control::me_action(&desk, name, a),
        "preview" | "program" => control::bus(&desk, name, a),
        "transition" => control::transition(&desk, a),
        "dsk" => keys::dsk(&desk, a),
        "usk" => keys::usk(&desk, a),
        "aux" => keys::aux(&desk, a),
        "macro" => keys::macro_action(&mut desk, a),
        other => Err(RpcError::new(codes::METHOD_NOT_FOUND, format!("gmx-atem has no tool '{other}'. It has: {}.", NAMES.join(", ")))),
    }
}

/// A sentence and the facts.
pub fn answer(text: impl Into<String>, facts: Value) -> ToolResult {
    ToolResult { content: json!([{"type": "text", "text": text.into()}]), structured_content: Some(facts), is_error: None }
}

/// A client library error as a refusal with its code in `data`.
pub fn refuse(e: godwinmix_atem::Error) -> RpcError {
    use godwinmix_atem::Error::{Down, Unknown};
    let code = match e {
        Down { .. } => codes::WRONG_STATE,
        Unknown { .. } => codes::NOT_FOUND,
        _ => codes::INVALID_PARAMS,
    };
    RpcError::new(code, e.to_string()).with_data(json!({"code": e.code()}))
}

/// A refusal about the arguments.
pub fn bad(message: impl Into<String>) -> RpcError {
    RpcError::new(codes::INVALID_PARAMS, message.into())
}
