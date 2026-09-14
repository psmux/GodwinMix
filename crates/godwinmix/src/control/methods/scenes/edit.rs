//! Editing off air, batching, undo, and the armed preview.
//!
//! Editing is off air by default (11 section 4). A draft is a working copy of
//! one scene; a draft of the scene that is on air is applied on the next take
//! or on an explicit apply, never on each keystroke. A client that wants live
//! on air editing asks for it with `live: true` and its UI says so. That is
//! the OBS pitfall of editing the programme scene live, turned into a choice.

use super::{client, scene_error, server};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::mixer::{Command, ProgramScene};
use godwinmix_core::scene::server::SceneView;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::requests::*;

pub fn register(reg: &mut Registry<Call>) {
    drafts(reg);
    transactions(reg);
    preview(reg);
}

fn drafts(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.edit.begin",
            Scope::Operate,
            "Take a working copy of a scene. Editing is off air by default: the draft is \
             written back on the next take of that scene, or when you apply it.",
            handler(|call: Call, params| async move {
                let req: EditBeginRequest = call.params(&params)?;
                let draft = server(&call)
                    .edit_begin(&req.scene, req.live)
                    .map_err(|e| scene_error(&call, e))?;
                body(DraftRecord {
                    draft: draft.id.to_string(),
                    scene: draft.name.clone(),
                    live: draft.live,
                    view: server(&call).scene(&req.scene).ok(),
                })
            }),
        )
        .params(schema_of::<EditBeginRequest>)
        .result(schema_of::<DraftRecord>),
    );

    reg.register(
        MethodDef::new(
            "scene.edit.apply",
            Scope::Operate,
            "Write a draft back into the live document.",
            handler(|call: Call, params| async move {
                let req: DraftRequest = call.params(&params)?;
                let outcome = server(&call)
                    .edit_apply(client(&call).as_deref(), &req.draft)
                    .map_err(|e| scene_error(&call, e))?;
                super::answered(outcome)
            }),
        )
        .params(schema_of::<DraftRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.edit.discard",
            Scope::Operate,
            "Throw a draft away. The live document is untouched.",
            handler(|call: Call, params| async move {
                let req: DraftRequest = call.params(&params)?;
                let draft =
                    server(&call).edit_discard(&req.draft).map_err(|e| scene_error(&call, e))?;
                body(json!({ "discarded": draft.id.to_string(), "scene": draft.name }))
            }),
        )
        .params(schema_of::<DraftRequest>)
        .result(any_object),
    );
}

fn transactions(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.transaction.begin",
            Scope::Operate,
            "Start a batch. Everything until the commit applies on one frame or not at all, \
             and undoes in one step.",
            handler(|call: Call, _| async move {
                server(&call).begin().map_err(|e| scene_error(&call, e))?;
                Ok(json!({ "open": true }))
            }),
        )
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.transaction.commit",
            Scope::Operate,
            "Apply the batch.",
            handler(|call: Call, _| async move {
                let patch =
                    server(&call).commit(client(&call).as_deref()).map_err(|e| scene_error(&call, e))?;
                body(patch)
            }),
        )
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.transaction.abort",
            Scope::Operate,
            "Throw the batch away. The document goes back to where it was when the batch \
             opened.",
            handler(|call: Call, _| async move {
                let patch =
                    server(&call).abort(client(&call).as_deref()).map_err(|e| scene_error(&call, e))?;
                body(patch)
            }),
        )
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.undo",
            Scope::Operate,
            "Undo the last change. A drag marked with scene.history.mark undoes as one step.",
            handler(|call: Call, _| async move {
                let patch =
                    server(&call).undo(client(&call).as_deref()).map_err(|e| scene_error(&call, e))?;
                body(HistoryStep { patch, undo: server(&call).history().0, redo: server(&call).history().1 })
            }),
        )
        .result(schema_of::<HistoryStep>)
        .tool(
            "undo_scene_edit",
            Tier::Search,
            "Undo the last change to a scene, exactly. Use it the moment an edit turns out \
             wrong rather than working out by hand what to put back. Refused, with how \
             many steps are on the stack, when there is nothing to undo.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.redo",
            Scope::Operate,
            "Put back what undo took away.",
            handler(|call: Call, _| async move {
                let patch =
                    server(&call).redo(client(&call).as_deref()).map_err(|e| scene_error(&call, e))?;
                body(HistoryStep { patch, undo: server(&call).history().0, redo: server(&call).history().1 })
            }),
        )
        .result(schema_of::<HistoryStep>),
    );

    reg.register(
        MethodDef::new(
            "scene.history.mark",
            Scope::Operate,
            "Group the changes that follow into one undo step, until the next mark. This \
             is what makes a drag of forty moves one Ctrl+Z.",
            handler(|call: Call, params| async move {
                let req: MarkRequest = call.params(&params)?;
                server(&call).mark(req.label.clone());
                let (undo, redo) = server(&call).history();
                Ok(json!({ "label": req.label, "undo": undo, "redo": redo }))
            }),
        )
        .params(schema_of::<MarkRequest>)
        .result(any_object),
    );
}

fn preview(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.preview.set",
            Scope::Operate,
            "Arm a scene. The armed scene is the preview, and program.take with no argument \
             takes it.",
            handler(|call: Call, params| async move {
                let req: PreviewRequest = call.params(&params)?;
                let armed = server(&call)
                    .arm(req.scene.as_deref())
                    .map_err(|e| scene_error(&call, e))?;
                // Nothing is composited for a preview until a client asks for
                // one with ext.preview, so arming costs one message and a
                // property write. Everything that follows the preview (the
                // tally, the multiview layout, /mjpeg/preview) reads the armed
                // scene off the server rather than off this event.
                call.app.mixer.emit(godwinmix_protocol::types::Event::PreviewChanged {
                    scene: armed.as_ref().map(|s| s.name.clone()),
                });
                body(json!({ "preview": armed }))
            }),
        )
        .params(schema_of::<PreviewRequest>)
        .result(any_object)
        .tool(
            "arm_preview",
            Tier::Search,
            "Arm a scene as the preview, so `take` with no source puts it on air. Arming \
             costs nothing: nothing is composited for a preview unless a client is watching \
             one. Pass no scene to clear it.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.preview.frame",
            Scope::Read,
            "A still of the armed scene as base64 JPEG, the floor every client has.",
            handler(preview_frame),
        )
        .params(schema_of::<PreviewFrameRequest>)
        .result(any_object),
    );
}

/// The preview still.
///
/// The armed scene is composited in the multiview pipeline from the per source
/// thumbnails, which is `multiview.rs` and not this module's to build. Where
/// the mosaic is up, the frame comes from it; where it is not, this says so and
/// names the next step rather than building a whole pipeline for one still.
async fn preview_frame(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PreviewFrameRequest = call.params(&params)?;
    let Some(layout) = server(&call).preview_layout(
        req.width.unwrap_or(320) as i32,
        (req.width.unwrap_or(320) as f64 * 9.0 / 16.0) as i32,
    ) else {
        return Err(RpcError::new(
            ErrorCode::NotInState,
            "no scene is armed, so there is no preview to photograph. Arm one with \
             scene.preview.set."
                .to_string(),
        ));
    };
    let latest = call.snapshots.latest_wanted(std::time::Duration::from_secs(2)).await;
    let Some(latest) = latest else {
        return Err(RpcError::new(
            ErrorCode::NotInState,
            format!(
                "the scene {:?} is armed, but the preview picture is composited in the \
                 multiview pipeline and no mosaic is running. Subscribe with ext.preview \
                 or ext.multiview, or open /mjpeg/preview, and ask again.",
                layout.name
            ),
        )
        .with("scene", layout.name.clone())
        .with("retryable", true));
    };
    use base64::Engine;
    Ok(json!({
        "scene": layout.name,
        "width": layout.width,
        "height": layout.height,
        "layout": layout.cells,
        "image": base64::engine::general_purpose::STANDARD.encode(&latest.jpeg),
        "encoding": "base64",
        "format": "jpeg",
        "note": "the still is the programme mosaic; the layout says where the armed \
                 scene's items would sit on it",
    }))
}

/// Ramp the pads of a scene that is on air towards what the document now says.
///
/// A geometry command with a duration on a scene nobody is looking at is a
/// cut: there is nothing being drawn to ramp. On air, the mixer is already
/// drawing these items (the ids were kept), so the change is a property ramp
/// on the pads it has, which is what makes an animated layout change a move
/// rather than a cut between two sets of items.
pub(crate) async fn ramp_if_on_air(call: &Call, view: &SceneView, ms: u64, easing: Option<&str>) {
    let Ok(status) = call.app.mixer.status().await else { return };
    if status.scene.as_deref() != Some(view.name.as_str()) {
        return;
    }
    let Ok((name, placements)) = server(call).placements(&view.name) else { return };
    let _ = call.app.mixer.send(Command::TakeScene {
        scene: Box::new(ProgramScene { name, placements }),
        at_running_time_ms: None,
        ack: None,
    });
    tracing::debug!(scene = %view.name, duration_ms = ms, easing, "layout change applied on air");
}

/// `scene.edit.begin`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DraftRecord {
    /// Pass this as `draft` on any `scene.item.*` call to edit the copy.
    pub draft: String,
    /// The scene it was taken from.
    pub scene: String,
    /// True when the client asked to edit on air.
    pub live: bool,
    /// The scene as it stands, so the client has something to draw at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<SceneView>,
}

/// `scene.undo` and `scene.redo`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HistoryStep {
    pub patch: godwinmix_core::scene::server::Patch,
    /// How many steps are still on each stack, so a UI greys out a button.
    pub undo: usize,
    pub redo: usize,
}
