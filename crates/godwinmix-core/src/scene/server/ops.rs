//! The operations an agent and a keyboard user reach for.
//!
//! Each one is a single call that cannot produce an off by twelve pixels
//! result (11 section 4). They are pure functions over a `Collection`: the
//! server wraps them in a transaction, a patch and an undo step, and nothing
//! here knows about any of that.
//!
//! Everything works in canvas pixels through `scene::geometry`, which is the
//! one place the arithmetic lives.

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;

use super::find;
use crate::scene::document::{
    Align, Collection, Content, Crop, Fit, Frame, Item, Scene, Transform, Vec2,
};
use crate::scene::geometry::{self, Rect};
use crate::scene::id::Id;
use crate::scene::order;

/// Which edge to line items up on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    CenterX,
    CenterY,
}

impl Edge {
    pub fn parse(s: &str) -> Result<Edge> {
        Ok(match s.trim().to_lowercase().replace('_', "-").as_str() {
            "left" => Edge::Left,
            "right" => Edge::Right,
            "top" => Edge::Top,
            "bottom" => Edge::Bottom,
            "center-x" | "centre-x" | "center" | "centre" | "middle" => Edge::CenterX,
            "center-y" | "centre-y" => Edge::CenterY,
            other => bail!(
                "{other:?} is not an edge. Use left, right, top, bottom, center-x or center-y"
            ),
        })
    }
}

/// Which way to spread items out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub fn parse(s: &str) -> Result<Axis> {
        Ok(match s.trim().to_lowercase().as_str() {
            "horizontal" | "h" | "x" => Axis::Horizontal,
            "vertical" | "v" | "y" => Axis::Vertical,
            other => bail!("{other:?} is not an axis. Use horizontal or vertical"),
        })
    }
}

/// Every item in a scene, flattened, with the box it occupies.
///
/// Group children are included, because aligning two items that happen to be
/// in different groups is a thing a person asks for and the answer should not
/// depend on where they happen to sit in the tree.
fn boxes(doc: &Collection, scene: &Scene) -> BTreeMap<Id, Rect> {
    let resolved = super::compose::resolve(doc, scene);
    geometry::flatten(&resolved, &doc.canvas)
        .into_iter()
        .map(|p| (p.item.id, p.rect))
        .collect()
}

/// Move one item so its box lands where `to` says, whatever the tree above it
/// is doing.
///
/// The item's own transform is what changes, never its parent's and never a
/// sibling's: that is the OBS group corruption class, and doing the arithmetic
/// once here is what keeps it out.
fn move_to(scene: &mut Scene, id: Id, current: Rect, to: (f64, f64)) {
    let delta = (to.0 - current.x, to.1 - current.y);
    if let Some(item) = item_mut(&mut scene.items, id) {
        item.transform.position.x += delta.0;
        item.transform.position.y += delta.1;
    }
}

/// A mutable reference to an item anywhere in the tree.
pub fn item_mut(items: &mut [Item], id: Id) -> Option<&mut Item> {
    for item in items.iter_mut() {
        if item.id == id {
            return Some(item);
        }
        if let Content::Children { children } = &mut item.content {
            if let Some(found) = item_mut(children, id) {
                return Some(found);
            }
        }
    }
    None
}

/// Take an item out of the tree and give it back.
pub fn take_item(items: &mut Vec<Item>, id: Id) -> Option<Item> {
    if let Some(pos) = items.iter().position(|i| i.id == id) {
        return Some(items.remove(pos));
    }
    for item in items.iter_mut() {
        if let Content::Children { children } = &mut item.content {
            if let Some(found) = take_item(children, id) {
                return Some(found);
            }
        }
    }
    None
}

/// Resolve every name the caller gave into an id in this scene.
pub fn ids(scene: &Scene, names: &[String]) -> Result<Vec<Id>> {
    names.iter().map(|n| find::item_id_in(scene, n)).collect()
}

/// Line items up on an edge.
pub fn align(doc: &mut Collection, index: usize, items: &[Id], edge: Edge) -> Result<()> {
    let rects = boxes(doc, &doc.scenes[index]);
    let chosen: Vec<(Id, Rect)> =
        items.iter().filter_map(|id| rects.get(id).map(|r| (*id, *r))).collect();
    if chosen.len() < 2 {
        bail!("aligning needs at least two items; {} were named and found", chosen.len());
    }
    let target = match edge {
        Edge::Left => chosen.iter().map(|(_, r)| r.x).fold(f64::MAX, f64::min),
        Edge::Right => chosen.iter().map(|(_, r)| r.right()).fold(f64::MIN, f64::max),
        Edge::Top => chosen.iter().map(|(_, r)| r.y).fold(f64::MAX, f64::min),
        Edge::Bottom => chosen.iter().map(|(_, r)| r.bottom()).fold(f64::MIN, f64::max),
        Edge::CenterX => {
            chosen.iter().map(|(_, r)| r.x + r.w / 2.0).sum::<f64>() / chosen.len() as f64
        }
        Edge::CenterY => {
            chosen.iter().map(|(_, r)| r.y + r.h / 2.0).sum::<f64>() / chosen.len() as f64
        }
    };
    let scene = &mut doc.scenes[index];
    for (id, rect) in chosen {
        let to = match edge {
            Edge::Left => (target, rect.y),
            Edge::Right => (target - rect.w, rect.y),
            Edge::Top => (rect.x, target),
            Edge::Bottom => (rect.x, target - rect.h),
            Edge::CenterX => (target - rect.w / 2.0, rect.y),
            Edge::CenterY => (rect.x, target - rect.h / 2.0),
        };
        move_to(scene, id, rect, to);
    }
    Ok(())
}

/// Space items evenly between the two on the ends.
pub fn distribute(doc: &mut Collection, index: usize, items: &[Id], axis: Axis) -> Result<()> {
    let rects = boxes(doc, &doc.scenes[index]);
    let mut chosen: Vec<(Id, Rect)> =
        items.iter().filter_map(|id| rects.get(id).map(|r| (*id, *r))).collect();
    if chosen.len() < 3 {
        bail!(
            "distributing needs at least three items: the two on the ends stay where they are and \
             the rest are spaced between them. {} were named and found",
            chosen.len()
        );
    }
    let key = |r: &Rect| if axis == Axis::Horizontal { r.x } else { r.y };
    chosen.sort_by(|a, b| key(&a.1).partial_cmp(&key(&b.1)).unwrap_or(std::cmp::Ordering::Equal));

    // Even gaps, not even positions: items of different sizes look wrong when
    // their corners are evenly spaced and right when the space between them is.
    let span = match axis {
        Axis::Horizontal => chosen.last().unwrap().1.right() - chosen[0].1.x,
        Axis::Vertical => chosen.last().unwrap().1.bottom() - chosen[0].1.y,
    };
    let filled: f64 = chosen
        .iter()
        .map(|(_, r)| if axis == Axis::Horizontal { r.w } else { r.h })
        .sum();
    let gap = (span - filled) / (chosen.len() - 1) as f64;

    let mut cursor = key(&chosen[0].1);
    let scene = &mut doc.scenes[index];
    for (id, rect) in &chosen {
        let to = match axis {
            Axis::Horizontal => (cursor, rect.y),
            Axis::Vertical => (rect.x, cursor),
        };
        move_to(scene, *id, *rect, to);
        cursor += if axis == Axis::Horizontal { rect.w } else { rect.h } + gap;
    }
    Ok(())
}

/// Put an item over the whole canvas, keeping its aspect ratio inside it.
pub fn fit_to_canvas(doc: &mut Collection, index: usize, items: &[Id]) -> Result<()> {
    cover_or_fit(doc, index, items, Fit::Contain)
}

/// Put an item over the whole canvas, filling it and letting the overflow go.
pub fn cover_canvas(doc: &mut Collection, index: usize, items: &[Id]) -> Result<()> {
    cover_or_fit(doc, index, items, Fit::Cover)
}

fn cover_or_fit(doc: &mut Collection, index: usize, items: &[Id], fit: Fit) -> Result<()> {
    let canvas = doc.canvas;
    let scene = &mut doc.scenes[index];
    for id in items {
        let Some(item) = item_mut(&mut scene.items, *id) else { continue };
        item.transform.position = Vec2::ZERO;
        item.transform.anchor = Vec2::ZERO;
        item.transform.scale = Vec2::ONE;
        item.transform.frame = Some(Frame::new(canvas.width as f64, canvas.height as f64));
        item.transform.fit = fit;
    }
    Ok(())
}

/// Lay items out in a grid of `cols` columns, filling the canvas.
pub fn arrange_grid(doc: &mut Collection, index: usize, items: &[Id], cols: usize) -> Result<()> {
    if items.is_empty() {
        bail!("arrange_grid needs at least one item");
    }
    let cols = cols.max(1);
    let rows = items.len().div_ceil(cols);
    let canvas = doc.canvas;
    let (w, h) = (canvas.width as f64 / cols as f64, canvas.height as f64 / rows as f64);
    let scene = &mut doc.scenes[index];
    for (i, id) in items.iter().enumerate() {
        let Some(item) = item_mut(&mut scene.items, *id) else { continue };
        item.transform.position = Vec2::new((i % cols) as f64 * w, (i / cols) as f64 * h);
        item.transform.anchor = Vec2::ZERO;
        item.transform.scale = Vec2::ONE;
        item.transform.frame = Some(Frame::new(w, h));
        if item.transform.fit == Fit::None {
            item.transform.fit = Fit::Cover;
        }
    }
    Ok(())
}

/// Make items the same size as another one.
pub fn match_size(doc: &mut Collection, index: usize, items: &[Id], to: Id) -> Result<()> {
    let rects = boxes(doc, &doc.scenes[index]);
    let target = *rects
        .get(&to)
        .with_context(|| format!("the item to match, {to}, is not placed in this scene"))?;
    let scene = &mut doc.scenes[index];
    for id in items.iter().filter(|id| **id != to) {
        let Some(item) = item_mut(&mut scene.items, *id) else { continue };
        item.transform.scale = Vec2::ONE;
        item.transform.frame = Some(Frame::new(target.w, target.h));
    }
    Ok(())
}

/// Put items into a group, at the position of the topmost one.
///
/// The group's transform is the identity and the children keep their own
/// transforms exactly as they were, so grouping changes nothing about the
/// picture. OBS wrote the group's box into each child and lost the child's own
/// position; a group here is an item with children and nothing else.
pub fn group(doc: &mut Collection, index: usize, items: &[Id], name: Option<String>) -> Result<Id> {
    if items.len() < 2 {
        bail!("grouping needs at least two items; {} were named", items.len());
    }
    let scene = &mut doc.scenes[index];
    let at = scene.items.iter().position(|i| items.contains(&i.id)).unwrap_or(scene.items.len());
    let mut children = Vec::new();
    for id in items {
        if let Some(item) = take_item(&mut scene.items, *id) {
            children.push(item);
        }
    }
    if children.len() < 2 {
        bail!("grouping needs at least two items in the same scene");
    }
    let mut group = Item::new(Content::Children { children });
    group.name = Some(find::free_name(scene, name.as_deref().unwrap_or("group")));
    let id = group.id;
    scene.items.insert(at.min(scene.items.len()), group);
    Ok(id)
}

/// Take a group apart, leaving every child where it looked.
///
/// The group's transform multiplies into each child on the way out, which is
/// the same arithmetic `geometry::compose` does at apply time. That is what
/// makes ungrouping invisible: the children were already being drawn there.
pub fn ungroup(doc: &mut Collection, index: usize, id: Id) -> Result<Vec<Id>> {
    let scene = &mut doc.scenes[index];
    let Some(group) = take_item(&mut scene.items, id) else {
        bail!("there is no item {id} in the scene {:?}", scene.name);
    };
    let Content::Children { children } = &group.content else {
        let name = group.name.clone().unwrap_or_else(|| id.to_string());
        scene.items.push(group);
        bail!("{name:?} is not a group, so there is nothing to ungroup");
    };
    let freed: Vec<Item> = children
        .iter()
        .map(|child| Item {
            transform: geometry::compose(&group.transform, &child.transform),
            opacity: group.opacity * child.opacity,
            visible: group.visible && child.visible,
            ..child.clone()
        })
        .collect();
    let ids = freed.iter().map(|i| i.id).collect();
    scene.items.extend(freed);
    Ok(ids)
}

/// Move an item in the stack. `before` and `after` are the neighbours it is
/// to land between, which is what fractional ordering is for: nothing else is
/// renumbered.
pub fn reorder(
    doc: &mut Collection,
    index: usize,
    id: Id,
    before: Option<Id>,
    after: Option<Id>,
) -> Result<()> {
    let scene = &mut doc.scenes[index];
    let Some(item) = take_item(&mut scene.items, id) else {
        bail!("there is no item {id} in the scene {:?}", scene.name);
    };
    let at = match (before, after) {
        (Some(b), _) => scene.items.iter().position(|i| i.id == b).unwrap_or(0),
        (None, Some(a)) => {
            scene.items.iter().position(|i| i.id == a).map(|p| p + 1).unwrap_or(scene.items.len())
        }
        (None, None) => scene.items.len(),
    };
    scene.items.insert(at.min(scene.items.len()), item);
    Ok(())
}

/// A scene's geometry, for copying onto another one.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Layout {
    /// The scene it came from, for a message.
    pub scene: String,
    pub canvas: crate::scene::document::Canvas,
    pub items: Vec<LayoutItem>,
}

/// One item's geometry: everything about where it sits and nothing about what
/// it shows.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct LayoutItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub transform: Transform,
    pub crop: Crop,
    pub opacity: f64,
    pub visible: bool,
}

/// Read the geometry off a scene.
pub fn copy_layout(scene: &Scene) -> Layout {
    Layout {
        scene: scene.name.clone(),
        canvas: crate::scene::document::Canvas::default(),
        items: scene
            .items
            .iter()
            .map(|i| LayoutItem {
                name: i.name.clone(),
                transform: i.transform,
                crop: i.crop,
                opacity: i.opacity,
                visible: i.visible,
            })
            .collect(),
    }
}

/// How a pasted layout finds the item it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    /// By item name first, then by slot order: "copy the design from one scene
    /// to another" in the shape 11 section 6b describes.
    Name,
    /// By position in the stack alone.
    Order,
}

impl Match {
    pub fn parse(s: &str) -> Result<Match> {
        Ok(match s.trim().to_lowercase().as_str() {
            "name" => Match::Name,
            "order" | "slot" => Match::Order,
            other => bail!("{other:?} is not a match rule. Use \"name\" or \"order\""),
        })
    }
}

/// Put one scene's geometry onto another's items. Unmatched items are left
/// exactly as they were.
pub fn paste_layout(scene: &mut Scene, layout: &Layout, how: Match) -> usize {
    let mut used = vec![false; layout.items.len()];
    let mut moved = 0;
    for (slot, item) in scene.items.iter_mut().enumerate() {
        let from = match how {
            Match::Name => item
                .name
                .as_ref()
                .and_then(|name| {
                    layout
                        .items
                        .iter()
                        .position(|l| l.name.as_ref() == Some(name) && !used[l_index(l, layout)])
                })
                .or_else(|| (slot < layout.items.len() && !used[slot]).then_some(slot)),
            Match::Order => (slot < layout.items.len()).then_some(slot),
        };
        let Some(from) = from else { continue };
        used[from] = true;
        let source = &layout.items[from];
        item.transform = source.transform;
        item.crop = source.crop;
        item.opacity = source.opacity;
        item.visible = source.visible;
        moved += 1;
    }
    moved
}

fn l_index(needle: &LayoutItem, layout: &Layout) -> usize {
    layout.items.iter().position(|l| std::ptr::eq(l, needle)).unwrap_or(0)
}

/// The built in layout for a set of sources, by count.
///
/// A drop of two source tiles on empty space makes a two box, not a dialog.
pub fn layout_for(count: usize) -> &'static str {
    match count {
        0 | 1 => "full",
        2 => "two-box",
        3 => "three-box",
        4 => "quad",
        _ => "grid",
    }
}

/// Make a scene out of a set of sources, laid out by count or by a named
/// layout, with the items named after the sources.
pub fn create_from(
    doc: &Collection,
    sources: &[String],
    layout: Option<&str>,
    name: Option<&str>,
) -> Result<Scene> {
    if sources.is_empty() {
        bail!("scene.create_from needs at least one source");
    }
    let wanted = layout.unwrap_or_else(|| layout_for(sources.len()));
    let mut scene = if wanted == "grid" {
        grid_scene(doc, sources)
    } else {
        let preset = crate::scene::layout::builtin(wanted)?;
        let slots = slot_names(&preset);
        if slots.len() < sources.len() {
            bail!(
                "the layout {wanted:?} has {} source slots ({}) and {} sources were given. Use a \
                 layout with more slots, or leave `layout` out and let the count pick one",
                slots.len(),
                slots.join(", "),
                sources.len()
            );
        }
        let values: crate::scene::layout::Values = slots
            .iter()
            .zip(sources)
            .map(|(slot, source)| (slot.clone(), serde_json::Value::from(source.clone())))
            .collect();
        crate::scene::layout::apply(&preset, &values, doc.canvas)?
    };
    scene.name = find::free_scene_name(doc, name.unwrap_or(&sources.join(" + ")));
    // Items named after their sources, because a model reasons about words.
    for item in scene.items.iter_mut() {
        if item.name.is_none() {
            if let Content::Source { source } = &item.content {
                item.name = Some(source.clone());
            }
        }
    }
    Ok(scene)
}

/// A grid of however many sources there are, for the counts no preset covers.
fn grid_scene(doc: &Collection, sources: &[String]) -> Scene {
    let cols = (sources.len() as f64).sqrt().ceil().max(1.0) as usize;
    let rows = sources.len().div_ceil(cols);
    let (w, h) = (doc.canvas.width as f64 / cols as f64, doc.canvas.height as f64 / rows as f64);
    let mut scene = Scene::new("grid");
    for (i, source) in sources.iter().enumerate() {
        let mut item = Item::new(Content::Source { source: source.clone() });
        item.name = Some(source.clone());
        item.transform = Transform {
            position: Vec2::new((i % cols) as f64 * w, (i / cols) as f64 * h),
            frame: Some(Frame::new(w, h)),
            fit: Fit::Cover,
            align: Align::Center,
            ..Transform::default()
        };
        scene.items.push(item);
    }
    scene
}

/// The `source` typed parameters of a layout preset, in the order it declares
/// them, which is the order sources are poured into it.
pub fn slot_names(preset: &Collection) -> Vec<String> {
    let Some(properties) = preset.params.get("properties").and_then(|p| p.as_object()) else {
        return Vec::new();
    };
    properties
        .iter()
        .filter(|(_, schema)| {
            schema.get("x-gmx-kind").and_then(|k| k.as_str()) == Some("source")
                || schema.get("type").and_then(|t| t.as_str()) == Some("string")
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// A fresh id for every node in a copied scene, so a duplicate shares nothing
/// with its original.
pub fn renumber(scene: &mut Scene) {
    scene.id = Id::new();
    renumber_items(&mut scene.items);
}

fn renumber_items(items: &mut [Item]) {
    for item in items.iter_mut() {
        item.id = Id::new();
        if let Content::Children { children } = &mut item.content {
            renumber_items(children);
        }
    }
}

/// Where a new item goes when the caller gave no transform: the next free cell
/// of a grid over what is already there, so a drop on a scene tile never needs
/// a dialog (11 section 6b).
pub fn next_free_cell(doc: &Collection, scene: &Scene) -> Transform {
    let used = scene.items.len();
    if used == 0 {
        return Transform {
            frame: Some(Frame::new(doc.canvas.width as f64, doc.canvas.height as f64)),
            fit: Fit::Cover,
            ..Transform::default()
        };
    }
    let cols = ((used + 1) as f64).sqrt().ceil().max(1.0) as usize;
    let rows = (used + 1).div_ceil(cols);
    let (w, h) = (doc.canvas.width as f64 / cols as f64, doc.canvas.height as f64 / rows as f64);
    Transform {
        position: Vec2::new((used % cols) as f64 * w, (used / cols) as f64 * h),
        frame: Some(Frame::new(w, h)),
        fit: Fit::Cover,
        ..Transform::default()
    }
}

/// The fractional order key for an item landing between two others, so
/// inserting never renumbers a sibling.
pub fn order_between(before: Option<&str>, after: Option<&str>) -> Option<String> {
    order::between(before, after)
}
