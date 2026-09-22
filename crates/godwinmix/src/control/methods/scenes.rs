//! The `scene.*` commands.
//!
//! Semantic first, coordinates available. Every one of these takes an item or a
//! scene by name as readily as by id, because an agent reasons about
//! `lower-third` and not about `item_4f2a`, and a miss answers with the names
//! that would have worked. Every mutating one returns the resulting records
//! plus the derived geometry, so a client draws handles and an agent checks its
//! own work without a second call.
//!
//! The work is all in `godwinmix_core::scene::server`. What is here is the
//! table: one row per method, the request type, and the turn from a refusal
//! into an error that names the next step.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use godwinmix_core::scene::server::{ops, Outcome, SceneServer};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

pub(crate) mod edit;
mod graphics;
mod items;
mod obs;
pub(crate) mod layout;
mod requests;
mod share;

pub use requests::*;

pub fn register(reg: &mut Registry<Call>) {
    scenes(reg);
    items::register(reg);
    layout::register(reg);
    graphics::register(reg);
    share::register(reg);
    edit::register(reg);
}

/// `scene.list`, `get`, `add`, `remove`, `rename`, `duplicate`, `validate`.
fn scenes(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.list",
            Scope::Read,
            "Every scene in the collection, with how many items it has, the sources it \
             draws and whether it is armed.",
            handler(|call: Call, _| async move {
                body(SceneListing { scenes: call.app.scenes.list() })
            }),
        )
        .result(schema_of::<SceneListing>)
        .tool(
            "list_scenes",
            Tier::Search,
            "Every scene this mixer has: its name, how many items are in it, which sources \
             it draws and whether it is armed for the next take. Start here before taking \
             a scene or editing one. Scenes are named, and every scene command takes the \
             name as readily as the id.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.get",
            Scope::Read,
            "One scene: its records and where every item actually lands on the canvas.",
            handler(|call: Call, params| async move {
                let req: SceneRequest = call.params(&params)?;
                body(call.app.scenes.scene(&req.scene).map_err(|e| scene_error(&call, e))?)
            }),
        )
        .params(schema_of::<SceneRequest>)
        .result(schema_of::<godwinmix_core::scene::server::SceneView>)
        .tool(
            "get_scene",
            Tier::Search,
            "One scene in full: every item with its name, what it shows, and the box it \
             lands in on the canvas in pixels, plus anything the validator would complain \
             about. Use it before moving something, so you know what is there.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.add",
            Scope::Operate,
            "Make an empty scene, or one built from a set of sources.",
            handler(add),
        )
        .params(schema_of::<AddSceneRequest>)
        .result(schema_of::<godwinmix_core::scene::server::SceneView>),
    );

    reg.register(
        MethodDef::new(
            "scene.create_from",
            Scope::Operate,
            "A scene from a set of sources, laid out by the built in layout for that count \
             (full, two-box, three-box, quad, then a grid) or by a named one.",
            handler(create_from),
        )
        .params(schema_of::<CreateFromRequest>)
        .result(schema_of::<godwinmix_core::scene::server::SceneView>)
        .tool(
            "create_scene_from",
            Tier::Search,
            "Make a scene out of a list of sources in one call. With no `layout` the count \
             picks one: one source fills the canvas, two make a two box, three a three \
             box, four a quad, more a grid. The items are named after their sources, so \
             you can move them by name afterwards. Returns the scene with every item's box \
             in pixels.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.remove",
            Scope::Operate,
            "Delete a scene. What is on air is not touched.",
            handler(remove),
        )
        .params(schema_of::<SceneRequest>)
        .result(schema_of::<SceneRemoved>)
        .destructive(),
    );

    reg.register(
        MethodDef::new(
            "scene.rename",
            Scope::Operate,
            "Change a scene's name, its colour, or both. Names and colours live on the \
             document, so every client, the tally and an agent see the same ones.",
            handler(rename),
        )
        .params(schema_of::<RenameSceneRequest>)
        .result(schema_of::<godwinmix_core::scene::server::SceneView>),
    );

    reg.register(
        MethodDef::new(
            "scene.duplicate",
            Scope::Operate,
            "A copy of a scene with new ids throughout, so editing the copy cannot touch \
             the original.",
            handler(duplicate),
        )
        .params(schema_of::<DuplicateSceneRequest>)
        .result(schema_of::<godwinmix_core::scene::server::SceneView>),
    );

    reg.register(
        MethodDef::new(
            "scene.validate",
            Scope::Read,
            "Overlaps, items off the canvas, safe area breaches and missing sources: what \
             to fix before saying a scene is done.",
            handler(|call: Call, params| async move {
                let req: ValidateRequest = call.params(&params)?;
                let findings = call
                    .app
                    .scenes
                    .validate(req.scene.as_deref())
                    .map_err(|e| scene_error(&call, e))?;
                body(Validation { ok: findings.is_empty(), findings })
            }),
        )
        .params(schema_of::<ValidateRequest>)
        .result(schema_of::<Validation>)
        .tool(
            "validate_scene",
            Tier::Search,
            "Check a scene for the mistakes that are easy to make and hard to see: items \
             off the canvas, text outside the title safe area, an item completely hidden \
             behind another, a reference to a scene that is not there. Run it before \
             saying a scene is finished. An empty `findings` list means there is nothing \
             to fix.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.export",
            Scope::Read,
            "The whole collection: as JSON, or as a zip bundle carrying its assets with a \
             hash each, which is what you send somebody.",
            handler(share::export),
        )
        .params(schema_of::<share::ExportRequest>)
        .result(godwinmix_protocol::method::any_object)
        .tool(
            "export_collection",
            Tier::Search,
            "This mixer's whole collection, to keep or to send somebody. \
             {format: \"json\"} is the scene document alone. {format: \"zip\", path: \
             \"/tmp/show.zip\"} writes a bundle carrying the assets, which is what \
             scene.import reads back.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.import.obs",
            Scope::Operate,
            "Read an OBS Studio scene collection and add its scenes to this one. Send the \
             file's text as `content` (what a page's file picker reads) or a `path` on the \
             mixer's machine. With `add_sources: true` the sources the scenes draw are added \
             through source.add, and the answer says which were added and why any were not.",
            handler(obs::import_obs),
        )
        .params(schema_of::<ImportObsRequest>)
        .result(schema_of::<obs::ImportReport>),
    );
}

/// The answer every mutating scene command gives: the scene as it now is.
pub(crate) fn answered(outcome: Outcome) -> Result<Value, RpcError> {
    match outcome.scene {
        Some(view) => body(view),
        None => Ok(json!({ "changed": !outcome.patch.is_empty() })),
    }
}

/// A refusal from the scene server already names the state and the next step
/// (the valid names, the open drafts, what a layout takes), so it passes
/// through rather than being rewritten.
pub(crate) fn scene_error(call: &Call, e: anyhow::Error) -> RpcError {
    let text = format!("{e:#}");
    let code = if text.contains("there is no") { ErrorCode::NotFound } else { ErrorCode::NotInState };
    RpcError::new(code, text).with("method", call.method)
}

/// The scene server, which every one of these needs.
pub(crate) fn server(call: &Call) -> &Arc<SceneServer> {
    &call.app.scenes
}

/// Who asked, so a client can suppress the echo of its own edits.
pub(crate) fn client(call: &Call) -> Option<String> {
    Some(call.token.id.clone())
}

async fn add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: AddSceneRequest = call.params(&params)?;
    let name = req.name.clone();
    let (id, _) = server(&call)
        .edit(client(&call).as_deref(), move |doc| {
            let mut scene = godwinmix_core::scene::Scene::new(
                godwinmix_core::scene::server::find::free_scene_name(doc, &name),
            );
            scene.color = req.color.clone();
            let id = scene.id;
            doc.scenes.push(scene);
            Ok(id)
        })
        .map_err(|e| scene_error(&call, e))?;
    body(server(&call).scene(&id.to_string()).map_err(|e| scene_error(&call, e))?)
}

async fn create_from(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: CreateFromRequest = call.params(&params)?;
    if req.sources.is_empty() {
        return Err(RpcError::invalid_params(
            "scene.create_from needs at least one source. Read the ids from source.list.",
        ));
    }
    // A source the mixer does not have would be a scene that draws nothing, so
    // it is refused here with the ids that would have worked.
    let known = call.source_ids().await?;
    if let Some(missing) = req.sources.iter().find(|s| !known.contains(s)) {
        return Err(RpcError::not_found("source", missing, &known));
    }
    let (id, _) = server(&call)
        .edit(client(&call).as_deref(), |doc| {
            let scene = ops::create_from(doc, &req.sources, req.layout.as_deref(), req.name.as_deref())?;
            let id = scene.id;
            doc.scenes.push(scene);
            Ok(id)
        })
        .map_err(|e| scene_error(&call, e))?;
    body(server(&call).scene(&id.to_string()).map_err(|e| scene_error(&call, e))?)
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SceneRequest = call.params(&params)?;
    if call.dry_run {
        let view = server(&call).scene(&req.scene).map_err(|e| scene_error(&call, e))?;
        return Ok(call.dry_run_answer(
            true,
            vec![format!("delete the scene {:?} and its {} items", view.name, view.geometry.len())],
        ));
    }
    let (name, _) = server(&call)
        .edit(client(&call).as_deref(), |doc| {
            let index = godwinmix_core::scene::server::find::scene_index(doc, &req.scene)?;
            Ok(doc.scenes.remove(index).name)
        })
        .map_err(|e| scene_error(&call, e))?;
    body(SceneRemoved { removed: name })
}

async fn rename(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: RenameSceneRequest = call.params(&params)?;
    let outcome = server(&call)
        .edit_scene(client(&call).as_deref(), &req.scene, |doc, i| {
            if let Some(name) = &req.name {
                doc.scenes[i].name = name.clone();
            }
            if let Some(color) = &req.color {
                doc.scenes[i].color = Some(color.clone());
            }
            Ok(())
        })
        .map_err(|e| scene_error(&call, e))?;
    answered(outcome)
}

async fn duplicate(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: DuplicateSceneRequest = call.params(&params)?;
    let (id, _) = server(&call)
        .edit(client(&call).as_deref(), |doc| {
            let index = godwinmix_core::scene::server::find::scene_index(doc, &req.scene)?;
            let mut copy = doc.scenes[index].clone();
            ops::renumber(&mut copy);
            copy.name = godwinmix_core::scene::server::find::free_scene_name(
                doc,
                req.name.as_deref().unwrap_or(&doc.scenes[index].name),
            );
            let id = copy.id;
            doc.scenes.push(copy);
            Ok(id)
        })
        .map_err(|e| scene_error(&call, e))?;
    body(server(&call).scene(&id.to_string()).map_err(|e| scene_error(&call, e))?)
}

/// `scene.list`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SceneListing {
    pub scenes: Vec<godwinmix_core::scene::server::SceneSummary>,
}

/// `scene.validate`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Validation {
    /// True when there is nothing to fix.
    pub ok: bool,
    pub findings: Vec<godwinmix_core::scene::validate::Finding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SceneRemoved {
    pub removed: String,
}
