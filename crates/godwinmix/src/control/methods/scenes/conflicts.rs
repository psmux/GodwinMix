//! A scene refusal that is about somebody else's change, as an error a client
//! can act on.
//!
//! The scene server refuses an undo or redo that would overwrite another
//! client's later edit, and a draft apply over a scene that changed after the
//! draft was taken. Both come out as `-32001` with the records in the way in
//! `data.conflicts`, each naming who changed it (`changed_by`, a client id)
//! and, while that client is connected, how a person would recognise it
//! (`who`: its label or its device). `data.retry` is the same call with
//! `force: true`, for a button that says "do it anyway".

use crate::control::call::Call;
use godwinmix_core::scene::server::{Conflict, Refused, Stale};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use serde_json::{json, Value};

/// The error for `e`, when it is one of these two refusals.
pub(super) fn refusal(call: &Call, e: &anyhow::Error) -> Option<RpcError> {
    if let Some(r) = e.downcast_ref::<Refused>() {
        return Some(
            base(call, e, "undo", &r.conflicts)
                .with("verb", r.verb)
                .with("next", json!([
                    format!("scene.{} with force: true puts your version back over theirs", r.verb),
                    "or change the item by hand, and your later changes still undo first",
                ])),
        );
    }
    let s = e.downcast_ref::<Stale>()?;
    Some(
        base(call, e, "draft", &s.changes)
            .with("draft", s.draft.to_string())
            .with("scene", s.scene.clone())
            .with("base_seq", s.base_seq)
            .with("seq", s.seq)
            .with("removed", s.removed)
            .with("next", json!([
                "scene.edit.discard then scene.edit.begin starts again from the scene as it is now",
                "scene.edit.apply with force: true replaces the scene with your draft",
            ])),
    )
}

fn base(call: &Call, e: &anyhow::Error, kind: &str, conflicts: &[Conflict]) -> RpcError {
    let listed: Vec<Value> = conflicts.iter().map(|c| described(call, c)).collect();
    RpcError::new(ErrorCode::NotInState, e.to_string())
        .with("method", call.method)
        .with("conflict", kind)
        .with("conflicts", listed)
        .with("retry", json!({ "method": call.method, "force": true }))
        .with("retryable", false)
}

/// One conflict, with a name for whoever is in the way when they are here.
fn described(call: &Call, c: &Conflict) -> Value {
    let mut value = serde_json::to_value(c).unwrap_or(Value::Null);
    let who = c.changed_by.as_deref().and_then(|id| call.app.presence.who(id));
    if let (Some(map), Some(who)) = (value.as_object_mut(), who) {
        map.insert("who".into(), json!(who));
    }
    value
}
