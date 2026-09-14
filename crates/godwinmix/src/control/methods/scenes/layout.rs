//! Layouts, parameters, and the source labels the tray works from.
//!
//! A layout preset is a scene whose sources are parameters, so applying one is
//! resolving the parameters and writing the result. Applying it onto a scene
//! that already exists keeps the item ids, which is why an animated layout
//! change is a property ramp on the same items rather than a cut between two
//! sets of them.

use super::{answered, client, scene_error, server};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::scene::layout;
use godwinmix_core::scene::server::{find, ops};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::requests::*;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.layout.list",
            Scope::Read,
            "The layouts that ship with the core, with the parameters each one takes.",
            handler(|_call: Call, _| async move { body(listing()) }),
        )
        .result(schema_of::<LayoutListing>)
        .tool(
            "list_layouts",
            Tier::Search,
            "The layouts this core ships with (full, the four pip corners, two-box, \
             three-box, quad, l-shape, split, multiview) and the parameters each takes: \
             which source goes in which slot, and any numbers like the inset size. Use it \
             before `apply_layout`.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.apply_layout",
            Scope::Operate,
            "Apply a layout, making a scene or reshaping one that exists. Applying onto an \
             existing scene keeps the item ids, so the change is a ramp and not a cut.",
            handler(apply_layout),
        )
        .params(schema_of::<ApplyLayoutRequest>)
        .result(any_object)
        .tool(
            "apply_layout",
            Tier::Search,
            "Put a layout on the canvas in one call: `{layout: \"pip-bottom-right\", \
             values: {a: \"cam1\", b: \"guest\"}}`. `values` names the layout's parameters, \
             which `list_layouts` tells you. `duration_ms` makes it a move rather than a \
             cut. Give `scene` to reshape a scene that already exists, which keeps its \
             items so the change animates.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.layout.copy",
            Scope::Read,
            "Read one scene's geometry, to paste onto another.",
            handler(|call: Call, params| async move {
                let req: SceneRequest = call.params(&params)?;
                let doc = server(&call).document();
                let scene = find::scene(&doc, &req.scene).map_err(|e| scene_error(&call, e))?;
                body(ops::copy_layout(scene))
            }),
        )
        .params(schema_of::<SceneRequest>)
        .result(schema_of::<ops::Layout>),
    );

    reg.register(
        MethodDef::new(
            "scene.layout.paste",
            Scope::Operate,
            "Put one scene's geometry onto another's items, matched by name first and slot \
             order second. Items that match nothing are left alone.",
            handler(paste),
        )
        .params(schema_of::<LayoutClipboardRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.params.get",
            Scope::Read,
            "The collection's typed parameters, readable without their values, so a client \
             discovers what is fillable before filling it.",
            handler(|call: Call, _| async move {
                let doc = server(&call).document();
                Ok(doc.params)
            }),
        )
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.params.set",
            Scope::Operate,
            "Set the collection's parameter values. A `{{name}}` in a string property \
             follows them.",
            handler(params_set),
        )
        .params(schema_of::<ParamsRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "source.set",
            Scope::Operate,
            "Name and colour a source. Both live on the scene document, so every client, \
             the tally and an agent see the same ones.",
            handler(source_set),
        )
        .params(schema_of::<SetSourceMetaRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "source.group",
            Scope::Operate,
            "Put sources in a tray folder. A tag for finding things, not a group on the \
             canvas.",
            handler(source_group),
        )
        .params(schema_of::<GroupSourcesRequest>)
        .result(any_object),
    );
}

async fn apply_layout(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ApplyLayoutRequest = call.params(&params)?;
    let preset = layout::builtin(&req.layout).map_err(|e| scene_error(&call, e))?;
    let values: layout::Values = req.values.clone().into_iter().collect();
    // Every source slot has to name a source this mixer has, or the layout
    // resolves into a scene that draws nothing.
    let known = call.source_ids().await?;
    for slot in ops::slot_names(&preset) {
        if let Some(Value::String(source)) = req.values.get(&slot) {
            if !known.contains(source) {
                return Err(RpcError::not_found("source", source, &known));
            }
        }
    }

    let (id, _) = server(&call)
        .edit(client(&call).as_deref(), |doc| {
            match &req.scene {
                // Onto a scene that exists: the same scene id and, through
                // `apply_into`, the same item ids, which is what makes the
                // change a property ramp rather than a cut between two sets of
                // items.
                Some(which) => {
                    let index = find::scene_index(doc, which)?;
                    let existing = doc.scenes[index].id;
                    let name = req.name.clone().unwrap_or_else(|| doc.scenes[index].name.clone());
                    let scene =
                        layout::apply_into(&preset, &values, doc.canvas, existing, Some(&name))?;
                    doc.scenes[index] = scene;
                    Ok(existing)
                }
                None => {
                    let mut scene = layout::apply(&preset, &values, doc.canvas)?;
                    scene.name = find::free_scene_name(
                        doc,
                        req.name.as_deref().unwrap_or(&req.layout),
                    );
                    let id = scene.id;
                    doc.scenes.push(scene);
                    Ok(id)
                }
            }
        })
        .map_err(|e| scene_error(&call, e))?;

    // A duration on a scene that is not on air is a cut either way: nothing is
    // being drawn to ramp. On air, the mixer ramps the pads it is already
    // drawing, which is the same items because the ids were kept.
    let view = server(&call).scene(&id.to_string()).map_err(|e| scene_error(&call, e))?;
    if let Some(ms) = req.duration_ms.filter(|ms| *ms > 0) {
        crate::control::methods::scenes::edit::ramp_if_on_air(&call, &view, ms, req.easing.as_deref())
            .await;
    }
    body(view)
}

async fn paste(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: LayoutClipboardRequest = call.params(&params)?;
    let Some(raw) = req.layout.clone() else {
        return Err(RpcError::invalid_params(
            "scene.layout.paste needs the `layout` that scene.layout.copy answered with.",
        ));
    };
    let layout: ops::Layout = serde_json::from_value(raw)
        .map_err(|e| RpcError::invalid_params(format!("that is not a layout: {e}")))?;
    let how = ops::Match::parse(req.r#match.as_deref().unwrap_or("name"))
        .map_err(|e| RpcError::invalid_params(format!("{e}")))?;
    let outcome = server(&call)
        .edit_scene(client(&call).as_deref(), &req.scene, |doc, i| {
            ops::paste_layout(&mut doc.scenes[i], &layout, how);
            Ok(())
        })
        .map_err(|e| scene_error(&call, e))?;
    answered(outcome)
}

async fn params_set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ParamsRequest = call.params(&params)?;
    let (value, _) = server(&call)
        .edit(client(&call).as_deref(), |doc| {
            let properties = doc
                .params
                .get_mut("properties")
                .and_then(|p| p.as_object_mut())
                .ok_or_else(|| anyhow::anyhow!("this collection declares no parameters"))?;
            for (key, value) in &req.values {
                let known: Vec<String> = properties.keys().cloned().collect();
                let schema = properties.get_mut(key).ok_or_else(|| {
                    anyhow::anyhow!(
                        "this collection has no parameter {key:?}. It has: {}",
                        known.join(", ")
                    )
                })?;
                schema
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("the parameter {key:?} is not an object"))?
                    .insert("default".into(), value.clone());
            }
            Ok(doc.params.clone())
        })
        .map_err(|e| scene_error(&call, e))?;
    Ok(value)
}

async fn source_set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SetSourceMetaRequest = call.params(&params)?;
    let known = call.source_ids().await?;
    if !known.contains(&req.source) {
        return Err(RpcError::not_found("source", &req.source, &known));
    }
    let (meta, _) = server(&call)
        .edit(client(&call).as_deref(), |doc| {
            let entry = doc.sources.entry(req.source.clone()).or_default();
            if let Some(name) = &req.name {
                entry.name = Some(name.clone());
            }
            if let Some(color) = &req.color {
                entry.color = Some(color.clone());
            }
            Ok(entry.clone())
        })
        .map_err(|e| scene_error(&call, e))?;
    body(SourceMetaRecord { source: req.source, meta })
}

async fn source_group(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GroupSourcesRequest = call.params(&params)?;
    let known = call.source_ids().await?;
    if let Some(missing) = req.sources.iter().find(|s| !known.contains(s)) {
        return Err(RpcError::not_found("source", missing, &known));
    }
    let (grouped, _) = server(&call)
        .edit(client(&call).as_deref(), |doc| {
            let mut out = Vec::new();
            for source in &req.sources {
                let entry = doc.sources.entry(source.clone()).or_default();
                entry.group = req.name.clone();
                out.push(SourceMetaRecord { source: source.clone(), meta: entry.clone() });
            }
            Ok(out)
        })
        .map_err(|e| scene_error(&call, e))?;
    body(SourceMetaListing { sources: grouped })
}

/// Every built in layout with what it takes.
fn listing() -> LayoutListing {
    LayoutListing {
        layouts: layout::NAMES
            .iter()
            .filter_map(|name| {
                let preset = layout::builtin(name).ok()?;
                Some(LayoutInfo {
                    name: (*name).to_string(),
                    description: preset.scenes.first().map(|s| s.name.clone()).unwrap_or_default(),
                    sources: ops::slot_names(&preset),
                    params: preset.params,
                })
            })
            .collect(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LayoutListing {
    pub layouts: Vec<LayoutInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LayoutInfo {
    pub name: String,
    /// What the layout calls its own scene, which is the nearest thing it has
    /// to a description.
    pub description: String,
    /// The parameters that take a source id, in the order sources are poured
    /// into them.
    pub sources: Vec<String>,
    /// The whole JSON Schema, so a client renders an inspector from it.
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceMetaRecord {
    pub source: String,
    #[serde(flatten)]
    pub meta: godwinmix_core::scene::SourceMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceMetaListing {
    pub sources: Vec<SourceMetaRecord>,
}
