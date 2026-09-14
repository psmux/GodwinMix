//! `scene.item.*`: the things that put something on the canvas and move it
//! around.
//!
//! Every one of these is a state assignment with an enum, never a verb pair
//! that can contradict itself, and every one accepts a draft id so the same
//! command edits a working copy or the live document without a second method.

use super::{answered, client, scene_error, server};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::scene::document::{Content, Item, Scene};
use godwinmix_core::scene::server::{find, ops, Outcome};
use godwinmix_core::scene::Collection;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

use super::requests::*;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.item.add",
            Scope::Operate,
            "Put something on a scene's canvas. With no transform it lands in the next \
             free cell, so a drop never needs a dialog.",
            handler(add),
        )
        .params(schema_of::<AddItemRequest>)
        .result(any_object)
        .tool(
            "add_scene_item",
            Tier::Search,
            "Put a source, another scene or a graphic onto a scene. `content` is \
             {\"source\": \"cam1\"} for a source. Leave `transform` out and it is placed in \
             the next free cell of a grid over what is already there. Name it something a \
             person would say, like \"lower third\", because every other command takes that \
             name.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.item.remove",
            Scope::Operate,
            "Take an item off a scene.",
            handler(remove),
        )
        .params(schema_of::<ItemRequest>)
        .result(any_object)
        .destructive(),
    );

    reg.register(
        MethodDef::new(
            "scene.item.set",
            Scope::Operate,
            "Assign an item's properties. Only the keys named move; the rest are left \
             alone, so calling it twice with the same body changes nothing the second time.",
            handler(set),
        )
        .params(schema_of::<SetItemRequest>)
        .result(any_object)
        .tool(
            "set_scene_item",
            Tier::Search,
            "Change one item on a scene: its position and size (`transform`), its crop, \
             its opacity, whether it is visible, whether its sound is heard (`audio`: \
             follow, always or never). Name the item by the name you gave it. `props` is a \
             partial object: what you leave out stays as it is. Returns the scene with \
             every item's box in pixels, so you can see where it landed.",
        ),
    );

    for (method, verb, description) in [
        (
            "scene.item.align",
            "align",
            "Line items up on an edge: left, right, top, bottom, center-x or center-y.",
        ),
        (
            "scene.item.distribute",
            "distribute",
            "Space items evenly between the two on the ends, horizontally or vertically.",
        ),
        (
            "scene.item.fit_to_canvas",
            "fit_to_canvas",
            "Put items over the whole canvas, keeping their aspect ratio inside it.",
        ),
        (
            "scene.item.cover_canvas",
            "cover_canvas",
            "Put items over the whole canvas, filling it and letting the overflow go.",
        ),
        ("scene.item.arrange_grid", "arrange_grid", "Lay items out in a grid of `cols` columns."),
        ("scene.item.match_size", "match_size", "Make items the same size as another one."),
        ("scene.item.group", "group", "Put items into a group. The picture does not change."),
    ] {
        let verb = verb.to_string();
        reg.register(
            MethodDef::new(method, Scope::Operate, description, handler(move |call, params| {
                let verb = verb.clone();
                async move { arrange(call, params, &verb).await }
            }))
            .params(schema_of::<ItemsRequest>)
            .result(any_object),
        );
    }

    reg.register(
        MethodDef::new(
            "scene.item.ungroup",
            Scope::Operate,
            "Take a group apart, leaving every child exactly where it looked.",
            handler(ungroup),
        )
        .params(schema_of::<ItemRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.item.reorder",
            Scope::Operate,
            "Move an item up or down the stack, between two named neighbours.",
            handler(reorder),
        )
        .params(schema_of::<ReorderRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.item.move",
            Scope::Operate,
            "Move an item to another scene, keeping its transform and filters.",
            handler(|call: Call, params| async move { transfer(call, params, false).await }),
        )
        .params(schema_of::<MoveItemRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.item.copy",
            Scope::Operate,
            "Copy an item into another scene. The copy keeps the transform and the filters \
             and gets a new id.",
            handler(|call: Call, params| async move { transfer(call, params, true).await }),
        )
        .params(schema_of::<MoveItemRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.item.bind",
            Scope::Operate,
            "Bind a geometry property to an expression over the collection's parameters, \
             so changing a number moves everything that follows it.",
            handler(bind),
        )
        .params(schema_of::<BindRequest>)
        .result(any_object),
    );

    filters(reg);
}

/// The per item filter chain. The document carries it; the mixer puts the
/// filter on the slot chain when the scene goes to air.
fn filters(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.item.filter.add",
            Scope::Operate,
            "Hang a filter on one item, so a camera keyed in one scene is not keyed in all \
             of them.",
            handler(filter_add),
        )
        .params(schema_of::<AddItemFilterRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.item.filter.set",
            Scope::Operate,
            "Change one of an item's filters, or turn it off without taking it out.",
            handler(filter_set),
        )
        .params(schema_of::<ItemFilterRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "scene.item.filter.remove",
            Scope::Operate,
            "Take a filter off an item.",
            handler(filter_remove),
        )
        .params(schema_of::<ItemFilterRequest>)
        .result(any_object)
        .destructive(),
    );
}

/// Run a change against a scene, or against a draft of one when the caller
/// named a draft. The one place that branch is made.
fn apply(
    call: &Call,
    scene: &str,
    draft: Option<&str>,
    f: impl FnOnce(&mut Collection, usize) -> anyhow::Result<()>,
) -> Result<Value, RpcError> {
    match draft {
        Some(draft) => {
            let view = server(call).edit_draft(draft, f).map_err(|e| scene_error(call, e))?;
            body(view)
        }
        None => {
            let outcome = server(call)
                .edit_scene(client(call).as_deref(), scene, f)
                .map_err(|e| scene_error(call, e))?;
            answered(outcome)
        }
    }
}

async fn add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: AddItemRequest = call.params(&params)?;
    let content: Content = serde_json::from_value(req.content.clone()).map_err(|e| {
        RpcError::invalid_params(format!(
            "`content` is {{\"source\": \"<id>\"}}, {{\"ref\": \"<scene id>\"}} or \
             {{\"graphic\": \"<plugin/id>\"}}: {e}"
        ))
    })?;
    // A source the mixer does not have draws nothing, so it is refused here
    // with the ids that would have worked.
    if let Content::Source { source } = &content {
        let known = call.source_ids().await;
        if !known.contains(source) {
            return Err(RpcError::not_found("source", source, &known));
        }
    }
    let transform = match &req.transform {
        Some(value) => Some(serde_json::from_value(value.clone()).map_err(|e| {
            RpcError::invalid_params(format!("`transform` is not a transform: {e}"))
        })?),
        None => None,
    };
    apply(&call, &req.scene, req.draft.as_deref(), move |doc, i| {
        let mut item = Item::new(content.clone());
        item.name = Some(find::free_name(
            &doc.scenes[i],
            req.name.as_deref().unwrap_or(&default_name(&content)),
        ));
        item.transform = transform.unwrap_or_else(|| ops::next_free_cell(doc, &doc.scenes[i]));
        doc.scenes[i].items.push(item);
        Ok(())
    })
}

/// What an item is called when nobody said: the source's own id, because a
/// model reasons about words and `cam1` is a word.
fn default_name(content: &Content) -> String {
    match content {
        Content::Source { source } => source.clone(),
        Content::Graphic { graphic, .. } => {
            graphic.rsplit('/').next().unwrap_or(graphic).to_string()
        }
        Content::Ref { .. } => "scene".into(),
        Content::Children { .. } => "group".into(),
    }
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ItemRequest = call.params(&params)?;
    if call.dry_run {
        let view = server(&call).scene(&req.scene).map_err(|e| scene_error(&call, e))?;
        return Ok(call.dry_run_answer(
            true,
            vec![format!("take {:?} off the scene {:?}", req.item, view.name)],
        ));
    }
    apply(&call, &req.scene, req.draft.as_deref(), move |doc, i| {
        let id = find::item_id_in(&doc.scenes[i], &req.item)?;
        ops::take_item(&mut doc.scenes[i].items, id);
        Ok(())
    })
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SetItemRequest = call.params(&params)?;
    let props = req.props.clone();
    apply(&call, &req.scene, req.draft.as_deref(), move |doc, i| {
        let id = find::item_id_in(&doc.scenes[i], &req.item)?;
        let item = ops::item_mut(&mut doc.scenes[i].items, id)
            .ok_or_else(|| anyhow::anyhow!("the item went away while it was being changed"))?;
        merge_props(item, &props)
    })
}

/// Assign the keys the caller named onto an item, leaving the rest alone.
///
/// Done by merging into the item's own JSON rather than by a match arm per
/// property, so a property added to the document type is settable the day it
/// exists and an unknown key is refused by the type rather than ignored.
fn merge_props(item: &mut Item, props: &serde_json::Map<String, Value>) -> anyhow::Result<()> {
    let mut value = serde_json::to_value(&*item)?;
    let object = value.as_object_mut().expect("an item is an object");
    for (key, v) in props {
        if key == "id" {
            anyhow::bail!("an item's id is assigned once and never changes");
        }
        // A nested object is merged rather than replaced, so setting
        // `transform.position` does not wipe the frame.
        match (object.get_mut(key), v) {
            (Some(Value::Object(existing)), Value::Object(new)) => {
                for (k, v) in new {
                    existing.insert(k.clone(), v.clone());
                }
            }
            _ => {
                object.insert(key.clone(), v.clone());
            }
        }
    }
    *item = serde_json::from_value(value).map_err(|e| {
        anyhow::anyhow!(
            "{e}. Settable keys are: name, content, transform, crop, opacity, blend, \
             visible, locked, audio, filters"
        )
    })?;
    Ok(())
}

async fn arrange(call: Call, params: Value, verb: &str) -> Result<Value, RpcError> {
    let req: ItemsRequest = call.params(&params)?;
    let verb = verb.to_string();
    apply(&call, &req.scene, req.draft.as_deref(), move |doc, i| {
        let ids = ops::ids(&doc.scenes[i], &req.items)?;
        match verb.as_str() {
            "align" => {
                let edge = ops::Edge::parse(req.edge.as_deref().unwrap_or("left"))?;
                ops::align(doc, i, &ids, edge)
            }
            "distribute" => {
                let axis = ops::Axis::parse(req.axis.as_deref().unwrap_or("horizontal"))?;
                ops::distribute(doc, i, &ids, axis)
            }
            "fit_to_canvas" => ops::fit_to_canvas(doc, i, &ids),
            "cover_canvas" => ops::cover_canvas(doc, i, &ids),
            "arrange_grid" => ops::arrange_grid(doc, i, &ids, req.cols.unwrap_or(2)),
            "match_size" => {
                let to = find::item_id_in(&doc.scenes[i], req.to.as_deref().unwrap_or_default())?;
                ops::match_size(doc, i, &ids, to)
            }
            "group" => ops::group(doc, i, &ids, req.name.clone()).map(|_| ()),
            other => anyhow::bail!("no such arrangement {other}"),
        }
    })
}

async fn ungroup(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ItemRequest = call.params(&params)?;
    apply(&call, &req.scene, req.draft.as_deref(), move |doc, i| {
        let id = find::item_id_in(&doc.scenes[i], &req.item)?;
        ops::ungroup(doc, i, id).map(|_| ())
    })
}

async fn reorder(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ReorderRequest = call.params(&params)?;
    apply(&call, &req.scene, req.draft.as_deref(), move |doc, i| {
        let id = find::item_id_in(&doc.scenes[i], &req.item)?;
        let before = req
            .before
            .as_deref()
            .map(|n| find::item_id_in(&doc.scenes[i], n))
            .transpose()?;
        let after =
            req.after.as_deref().map(|n| find::item_id_in(&doc.scenes[i], n)).transpose()?;
        ops::reorder(doc, i, id, before, after)
    })
}

async fn transfer(call: Call, params: Value, keep: bool) -> Result<Value, RpcError> {
    let req: MoveItemRequest = call.params(&params)?;
    let outcome = server(&call)
        .edit(client(&call).as_deref(), |doc| {
            let from = find::scene_index(doc, &req.scene)?;
            let to = find::scene_index(doc, &req.to_scene)?;
            if from == to {
                anyhow::bail!(
                    "{:?} is already in {:?}. Name a different scene to move it to",
                    req.item,
                    req.to_scene
                );
            }
            let id = find::item_id_in(&doc.scenes[from], &req.item)?;
            let mut item = if keep {
                let found = find::item_in(&doc.scenes[from], &req.item)?.clone();
                let mut copy = Scene { items: vec![found], ..Scene::new("copy") };
                ops::renumber(&mut copy);
                copy.items.remove(0)
            } else {
                ops::take_item(&mut doc.scenes[from].items, id)
                    .ok_or_else(|| anyhow::anyhow!("the item went away while it was being moved"))?
            };
            item.name = Some(find::free_name(
                &doc.scenes[to],
                &item.name.clone().unwrap_or_else(|| "item".into()),
            ));
            doc.scenes[to].items.push(item);
            Ok(doc.scenes[to].id)
        })
        .map_err(|e| scene_error(&call, e))?;
    body(server(&call).scene(&outcome.0.to_string()).map_err(|e| scene_error(&call, e))?)
}

async fn bind(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: BindRequest = call.params(&params)?;
    apply(&call, &req.scene, req.draft.as_deref(), move |doc, i| {
        let id = find::item_id_in(&doc.scenes[i], &req.item)?;
        let item = ops::item_mut(&mut doc.scenes[i].items, id)
            .ok_or_else(|| anyhow::anyhow!("the item went away while it was being bound"))?;
        if req.param.trim().is_empty() {
            item.bind.remove(&req.prop);
        } else {
            item.bind.insert(req.prop.clone(), req.param.clone());
        }
        Ok(())
    })
}

async fn filter_add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: AddItemFilterRequest = call.params(&params)?;
    let known = godwinmix_core::plugin::filter::available();
    if !known.contains(&req.type_id) {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!(
                "this build has no filter type {:?}. It has: {}",
                req.type_id,
                known.join(", ")
            ),
        ));
    }
    apply(&call, &req.scene, req.draft.as_deref(), move |doc, i| {
        let id = find::item_id_in(&doc.scenes[i], &req.item)?;
        let item = ops::item_mut(&mut doc.scenes[i].items, id)
            .ok_or_else(|| anyhow::anyhow!("the item went away"))?;
        item.filters.push(godwinmix_core::scene::Filter {
            kind: req.type_id.clone(),
            name: req.name.clone(),
            enabled: true,
            params: Value::Object(req.params.clone()),
        });
        Ok(())
    })
}

async fn filter_set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ItemFilterRequest = call.params(&params)?;
    apply(&call, &req.scene, req.draft.as_deref(), move |doc, i| {
        let id = find::item_id_in(&doc.scenes[i], &req.item)?;
        let item = ops::item_mut(&mut doc.scenes[i].items, id)
            .ok_or_else(|| anyhow::anyhow!("the item went away"))?;
        let at = filter_index(item, &req.filter)?;
        if let Some(enabled) = req.enabled {
            item.filters[at].enabled = enabled;
        }
        if !req.params.is_empty() {
            let existing = item.filters[at].params.as_object().cloned().unwrap_or_default();
            let mut merged = existing;
            for (k, v) in &req.params {
                merged.insert(k.clone(), v.clone());
            }
            item.filters[at].params = Value::Object(merged);
        }
        Ok(())
    })
}

async fn filter_remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ItemFilterRequest = call.params(&params)?;
    apply(&call, &req.scene, req.draft.as_deref(), move |doc, i| {
        let id = find::item_id_in(&doc.scenes[i], &req.item)?;
        let item = ops::item_mut(&mut doc.scenes[i].items, id)
            .ok_or_else(|| anyhow::anyhow!("the item went away"))?;
        let at = filter_index(item, &req.filter)?;
        item.filters.remove(at);
        Ok(())
    })
}

/// A filter by its name, its type, or its position in the chain.
fn filter_index(item: &Item, which: &str) -> anyhow::Result<usize> {
    let key = which.trim();
    if let Ok(n) = key.parse::<usize>() {
        if n < item.filters.len() {
            return Ok(n);
        }
    }
    item.filters
        .iter()
        .position(|f| f.name.as_deref() == Some(key) || f.kind == key)
        .ok_or_else(|| {
            let names: Vec<String> = item
                .filters
                .iter()
                .enumerate()
                .map(|(i, f)| f.name.clone().unwrap_or_else(|| format!("{i} ({})", f.kind)))
                .collect();
            anyhow::anyhow!(
                "that item has no filter {key:?}. It has: {}",
                if names.is_empty() { "none".into() } else { names.join(", ") }
            )
        })
}

/// So `Outcome` is used from this module's signature without a warning.
const _: Option<Outcome> = None;
