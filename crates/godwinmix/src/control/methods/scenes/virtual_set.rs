//! `scene.virtual_set`: a presenter keyed into a designed studio, in one call.
//!
//! The scene itself is the `virtual-set` layout, which anybody can apply with
//! `scene.create_from` or `scene.apply_layout`. What this adds is what a
//! layout cannot do on its own: turn a picture from the media library into a
//! source, and make a first guess at the key colour from the camera before
//! the scene exists, so the document holds a real colour rather than "auto".
//!
//! ```text
//!   scene.virtual_set {background, presenter, foreground?}
//!        |
//!        +-- source_for    a source id is used as it is; a media file or a
//!        |                 path becomes a source, once
//!        +-- key::guess    the screen colour, from a still of the camera
//!        \-- layout        virtual-set, applied as a new scene
//! ```

use super::{client, scene_error, server, set_inputs};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::scene::layout;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.virtual_set",
            Scope::Operate,
            "A new scene with a presenter keyed in front of a background, and optionally a \
             foreground such as a desk and a lower third area. Pictures from the media \
             library become sources; the key colour is guessed from the camera.",
            handler(create),
        )
        .params(schema_of::<VirtualSetRequest>)
        .result(schema_of::<VirtualSetAnswer>)
        .not_idempotent()
        .tool(
            "create_virtual_set",
            Tier::Search,
            "Put a presenter standing in front of a green or blue screen into a designed \
             studio, in one call. `background` is a picture or looping clip: a media \
             library file name from `list_media`, a path, or a source id. `presenter` is \
             the camera's source id. `foreground` is an optional transparent PNG in front \
             of the presenter, such as a desk. The key colour is guessed from the camera \
             unless you give `key` as \"#rrggbb\". Returns the scene and the colour used. \
             Then take a snapshot of the programme, and adjust the key with \
             `scene.item.filter.set` on the item `presenter` (similarity, smoothness, \
             spill, feather, matte_left and the other matte edges) and its size with \
             `scene.item.set`.",
        ),
    );
    super::key_color::register(reg);
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VirtualSetRequest {
    /// The new scene's name. "Virtual set" when left out, with a number after
    /// it when that is taken.
    #[serde(default)]
    pub name: Option<String>,
    /// What stands behind the presenter: a source id, a file name from the
    /// media library, or a path or URL to a picture or clip.
    pub background: String,
    /// The camera to key: a source id.
    pub presenter: String,
    /// A transparent picture in front of the presenter, drawn over the whole
    /// canvas: a source id, a media file name, or a path.
    #[serde(default)]
    pub foreground: Option<String>,
    /// A source for the lower third area, in front of everything else.
    #[serde(default)]
    pub lower_third: Option<String>,
    /// "auto" (the default) guesses the colour from the camera; "#rrggbb"
    /// gives it.
    #[serde(default)]
    pub key: Option<String>,
    /// The presenter's picture as a fraction of the canvas, 0.3 to 1. 0.9
    /// when left out.
    #[serde(default)]
    pub presenter_scale: Option<f64>,
    /// Where the presenter stands across the canvas, 0 to 1. 0.5, the
    /// middle, when left out.
    #[serde(default)]
    pub presenter_x: Option<f64>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct VirtualSetAnswer {
    pub scene: godwinmix_core::scene::server::SceneView,
    /// The key colour written on the presenter's key: "#rrggbb", or "auto"
    /// when no still of the camera could be had and the key finds it on air.
    pub key: String,
    /// "given", "guessed" or "auto".
    pub key_from: String,
    /// Sources this call added for files it was given.
    pub added: Vec<String>,
}

async fn create(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: VirtualSetRequest = call.params(&params)?;
    set_inputs::check_ranges(req.presenter_scale, req.presenter_x)?;
    let known = call.source_ids().await?;
    let presenter = req.presenter.trim().to_string();
    if !known.contains(&presenter) {
        return Err(RpcError::not_found("source", &presenter, &known)
            .with("field", "presenter")
            .with("next", "Add the camera with source.add first, then name its id here."));
    }
    let mut added = Vec::new();
    let background = set_inputs::source_for(&call, &known, &req.background, "background", &mut added).await?;
    let mut foreground = None;
    if let Some(f) = req.foreground.as_deref().filter(|f| !f.trim().is_empty()) {
        foreground = Some(set_inputs::source_for(&call, &known, f, "foreground", &mut added).await?);
    }
    let mut lower = None;
    if let Some(l) = req.lower_third.as_deref().filter(|l| !l.trim().is_empty()) {
        lower = Some(set_inputs::source_for(&call, &known, l, "lower_third", &mut added).await?);
    }
    let (key, key_from) = set_inputs::key_for(&call, &presenter, req.key.as_deref()).await?;

    let mut values = layout::Values::new();
    values.insert("a".into(), json!(background));
    values.insert("b".into(), json!(presenter));
    values.insert("c".into(), json!(foreground.unwrap_or_default()));
    values.insert("d".into(), json!(lower.unwrap_or_default()));
    values.insert("key".into(), json!(key));
    values.insert("presenter_scale".into(), json!(req.presenter_scale.unwrap_or(0.9)));
    values.insert("presenter_x".into(), json!(req.presenter_x.unwrap_or(0.5)));
    let preset = layout::builtin("virtual-set").map_err(|e| scene_error(&call, e))?;
    let name = req.name.clone().unwrap_or_else(|| "Virtual set".to_string());
    let (id, _) = server(&call)
        .edit(client(&call).as_deref(), move |doc| {
            let mut scene = layout::apply(&preset, &values, doc.canvas)?;
            scene.name = godwinmix_core::scene::server::find::free_scene_name(doc, &name);
            let id = scene.id;
            doc.scenes.push(scene);
            Ok(id)
        })
        .map_err(|e| scene_error(&call, e))?;
    let scene = server(&call).scene(&id.to_string()).map_err(|e| scene_error(&call, e))?;
    body(VirtualSetAnswer { scene, key, key_from: key_from.into(), added })
}
