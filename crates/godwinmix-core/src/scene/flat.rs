//! The flat projection of a scene document.
//!
//! The tree in `document.rs` is what a person reads. This is what the core and
//! the wire carry: one record per scene and per item, each with an id, a
//! parent, a fractional order and its typed props. Moving an item writes one
//! record; inserting between two siblings renumbers nothing; two clients
//! editing two items never touch the same path. That is tldraw's record store
//! and Excalidraw's fractional indexing, and it is why `event/scene.patch` can
//! be a patch rather than a snapshot.
//!
//! The two projections are lossless in both directions, and a test asserts it.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::document::*;
use super::id::Id;
use super::order;

/// The whole document as records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlatDocument {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub id: Id,
    pub name: String,
    pub canvas: Canvas,
    #[serde(default = "empty_params")]
    pub params: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<Transition>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<Id, Asset>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, SourceMeta>,
    /// Scenes and items together, in no particular order: `order` decides.
    pub records: Vec<Record>,
}

/// One scene or one item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Record {
    pub id: Id,
    /// The scene this item is in, or the group item it is a child of. Absent
    /// for a scene, which hangs off the document itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Id>,
    /// A fractional key. Siblings sort by it; see `order.rs`.
    pub order: String,
    #[serde(flatten)]
    pub props: Props,
}

/// What kind of record this is, and everything that belongs to it alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[allow(clippy::large_enum_variant)]
// An item's props are much bigger than a scene's name, and boxing them would
// buy an allocation per record on the hot path of every patch for a few bytes
// in the rare scene record. The records are read and written far more often
// than they are moved.
pub enum Props {
    Scene {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
    Item(ItemProps),
}

/// An item's props, which is an `Item` with the children lifted out into their
/// own records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemProps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub content: FlatContent,
    pub transform: Transform,
    pub crop: Crop,
    pub opacity: f64,
    pub blend: Blend,
    pub visible: bool,
    pub locked: bool,
    pub audio: Audio,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bind: BTreeMap<String, String>,
}

/// Content on the wire. The same four shapes as the tree, except that a group
/// names no children: they are records whose parent is the group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FlatContent {
    Source {
        source: String,
    },
    #[serde(rename = "ref")]
    Reference {
        #[serde(rename = "ref")]
        scene: Id,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        overrides: BTreeMap<Id, Override>,
    },
    Graphic {
        graphic: String,
        #[serde(default, skip_serializing_if = "Value::is_null")]
        params: Value,
    },
    Group,
}

impl Collection {
    /// The document as records.
    pub fn to_flat(&self) -> FlatDocument {
        let mut records = Vec::new();
        for (scene, order) in self.scenes.iter().zip(order::spread(self.scenes.len())) {
            records.push(Record {
                id: scene.id,
                parent: None,
                order,
                props: Props::Scene { name: scene.name.clone(), color: scene.color.clone() },
            });
            push_items(&scene.items, scene.id, &mut records);
        }
        FlatDocument {
            schema_version: self.schema_version,
            id: self.id,
            name: self.name.clone(),
            canvas: self.canvas,
            params: self.params.clone(),
            transitions: self.transitions.clone(),
            assets: self.assets.clone(),
            sources: self.sources.clone(),
            records,
        }
    }
}

/// Write one level of items as records, then recurse into the groups.
fn push_items(items: &[Item], parent: Id, out: &mut Vec<Record>) {
    for (item, order) in items.iter().zip(order::spread(items.len())) {
        out.push(Record {
            id: item.id,
            parent: Some(parent),
            order,
            props: Props::Item(ItemProps {
                name: item.name.clone(),
                content: FlatContent::from(&item.content),
                transform: item.transform,
                crop: item.crop,
                opacity: item.opacity,
                blend: item.blend,
                visible: item.visible,
                locked: item.locked,
                audio: item.audio,
                filters: item.filters.clone(),
                bind: item.bind.clone(),
            }),
        });
        push_items(item.children(), item.id, out);
    }
}

impl From<&Content> for FlatContent {
    fn from(c: &Content) -> FlatContent {
        match c {
            Content::Source { source } => FlatContent::Source { source: source.clone() },
            Content::Ref { scene, overrides } => {
                FlatContent::Reference { scene: *scene, overrides: overrides.clone() }
            }
            Content::Graphic { graphic, params } => {
                FlatContent::Graphic { graphic: graphic.clone(), params: params.clone() }
            }
            Content::Children { .. } => FlatContent::Group,
        }
    }
}

impl FlatDocument {
    /// Read records from JSON text.
    pub fn from_json(text: &str) -> Result<FlatDocument> {
        let mut value: Value = serde_json::from_str(text)?;
        super::migrate::migrate(&mut value)?;
        Ok(serde_json::from_value(value)?)
    }

    /// Records as JSON text, one property per line.
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("records always serialise");
        s.push('\n');
        s
    }

    /// The document as a tree again.
    ///
    /// Refuses a record whose parent is not in the store, and a parent chain
    /// that comes back to itself, because either one would otherwise drop items
    /// on the floor silently.
    pub fn to_tree(&self) -> Result<Collection> {
        let mut children: BTreeMap<Option<Id>, Vec<&Record>> = BTreeMap::new();
        for record in &self.records {
            children.entry(record.parent).or_default().push(record);
        }
        for group in children.values_mut() {
            group.sort_by(|a, b| (&a.order, &a.id).cmp(&(&b.order, &b.id)));
        }
        let known: std::collections::BTreeSet<Id> = self.records.iter().map(|r| r.id).collect();
        for record in &self.records {
            if let Some(parent) = record.parent {
                if !known.contains(&parent) {
                    bail!(
                        "record {} names parent {parent}, which is not in this store. Add that record, or set the parent to the scene it belongs to.",
                        record.id
                    );
                }
            }
        }
        let mut scenes = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for record in children.get(&None).into_iter().flatten() {
            let Props::Scene { name, color } = &record.props else {
                bail!("record {} has no parent, so it must be a scene, but it is an item. Give it the id of the scene it sits in.", record.id);
            };
            seen.insert(record.id);
            let mut depth = 0;
            scenes.push(Scene {
                id: record.id,
                name: name.clone(),
                color: color.clone(),
                items: build_items(record.id, &children, &mut depth, &mut seen)?,
            });
        }
        // Every record has to hang off a scene. One that does not is either a
        // leftover from a half applied patch or the tail of a parent loop, and
        // silently dropping it is how a client and the core stop agreeing.
        if let Some(lost) = self.records.iter().find(|r| !seen.contains(&r.id)) {
            bail!(
                "record {} is not reachable from any scene: its parent chain forms a loop, or the scene it belonged to is gone. Set its parent to a scene, or remove it.",
                lost.id
            );
        }
        let doc = Collection {
            schema_version: self.schema_version,
            id: self.id,
            name: self.name.clone(),
            canvas: self.canvas,
            params: self.params.clone(),
            scenes,
            transitions: self.transitions.clone(),
            assets: self.assets.clone(),
            sources: self.sources.clone(),
        };
        doc.check_refs()?;
        Ok(doc)
    }
}

/// The maximum group nesting a document may have. Deep enough that no real
/// scene meets it, shallow enough that a corrupt store cannot blow the stack.
const MAX_DEPTH: usize = 64;

/// Rebuild one level of the tree from the records under `parent`.
fn build_items(
    parent: Id,
    children: &BTreeMap<Option<Id>, Vec<&Record>>,
    depth: &mut usize,
    seen: &mut std::collections::BTreeSet<Id>,
) -> Result<Vec<Item>> {
    *depth += 1;
    if *depth > MAX_DEPTH {
        bail!("group nesting is more than {MAX_DEPTH} deep under {parent}, which means the parent links form a loop. Check the parent of each record in that group.");
    }
    let mut items = Vec::new();
    for record in children.get(&Some(parent)).into_iter().flatten() {
        let Props::Item(props) = &record.props else {
            bail!(
                "record {} is a scene but names parent {parent}. A scene has no parent; an item must have one.",
                record.id
            );
        };
        let content = match &props.content {
            FlatContent::Source { source } => Content::Source { source: source.clone() },
            FlatContent::Reference { scene, overrides } => {
                Content::Ref { scene: *scene, overrides: overrides.clone() }
            }
            FlatContent::Graphic { graphic, params } => {
                Content::Graphic { graphic: graphic.clone(), params: params.clone() }
            }
            FlatContent::Group => {
                Content::Children { children: build_items(record.id, children, depth, seen)? }
            }
        };
        seen.insert(record.id);
        items.push(Item {
            id: record.id,
            name: props.name.clone(),
            content,
            transform: props.transform,
            crop: props.crop,
            opacity: props.opacity,
            blend: props.blend,
            visible: props.visible,
            locked: props.locked,
            audio: props.audio,
            filters: props.filters.clone(),
            bind: props.bind.clone(),
        });
    }
    *depth -= 1;
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::layout;

    /// The same sample the document tests use, plus a reference and a graphic,
    /// so every content shape crosses the projection.
    fn sample() -> Collection {
        let mut doc = crate::scene::document::tests::sample();
        let lower = {
            let mut scene = Scene::new("Lower third");
            let mut item = Item::new(Content::Graphic {
                graphic: "lowerthird/graphic".into(),
                params: serde_json::json!({ "name": "{{speaker}}" }),
            });
            item.name = Some("name".into());
            item.audio = Audio::Never;
            scene.items.push(item);
            scene
        };
        let mut overrides = BTreeMap::new();
        overrides.insert(
            lower.items[0].id,
            Override { params: Some(serde_json::json!({ "name": "Ada" })), ..Override::default() },
        );
        let mut reference = Item::new(Content::Ref { scene: lower.id, overrides });
        reference.name = Some("lower third".into());
        doc.scenes[0].items.push(reference);
        doc.scenes.push(lower);
        doc
    }

    #[test]
    fn tree_to_flat_to_tree_is_lossless() {
        let doc = sample();
        let flat = doc.to_flat();
        assert_eq!(flat.to_tree().unwrap(), doc);
    }

    #[test]
    fn flat_to_tree_to_flat_is_lossless_through_json_too() {
        let flat = sample().to_flat();
        let text = flat.to_json();
        let read = FlatDocument::from_json(&text).unwrap();
        assert_eq!(read, flat);
        assert_eq!(read.to_tree().unwrap().to_flat(), flat);
    }

    #[test]
    fn every_built_in_layout_survives_the_round_trip() {
        for name in layout::NAMES {
            let doc = layout::builtin(name).unwrap();
            let back = doc.to_flat().to_tree().unwrap();
            assert_eq!(back, doc, "layout {name} did not survive the projection");
        }
    }

    #[test]
    fn a_group_keeps_its_children_in_order_and_only_its_own() {
        let flat = sample().to_flat();
        let groups: Vec<&Record> = flat
            .records
            .iter()
            .filter(|r| matches!(&r.props, Props::Item(p) if p.content == FlatContent::Group))
            .collect();
        assert_eq!(groups.len(), 1);
        let kids: Vec<&Record> =
            flat.records.iter().filter(|r| r.parent == Some(groups[0].id)).collect();
        assert_eq!(kids.len(), 1);
    }

    #[test]
    fn an_orphan_record_is_refused_and_the_message_says_what_to_do() {
        let mut flat = sample().to_flat();
        let item = flat
            .records
            .iter_mut()
            .find(|r| matches!(r.props, Props::Item(_)))
            .expect("a sample item");
        item.parent = Some(crate::scene::id::Id::new());
        let err = flat.to_tree().unwrap_err().to_string();
        assert!(err.contains("not in this store"), "{err}");
    }

    #[test]
    fn a_parent_loop_is_refused_rather_than_recursed_into() {
        let mut flat = sample().to_flat();
        let ids: Vec<Id> = flat
            .records
            .iter()
            .filter(|r| matches!(r.props, Props::Item(_)))
            .map(|r| r.id)
            .take(2)
            .collect();
        for record in flat.records.iter_mut() {
            if record.id == ids[0] {
                record.parent = Some(ids[1]);
            } else if record.id == ids[1] {
                record.parent = Some(ids[0]);
            }
        }
        let err = flat.to_tree().unwrap_err().to_string();
        assert!(err.contains("loop") || err.contains("not in this store"), "{err}");
    }

    #[test]
    fn record_order_and_not_array_position_decides_the_stack() {
        let doc = sample();
        let mut flat = doc.to_flat();
        flat.records.reverse();
        assert_eq!(flat.to_tree().unwrap(), doc, "shuffling the records changed the tree");
    }
}
