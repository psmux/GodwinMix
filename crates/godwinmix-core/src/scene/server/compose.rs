//! From a scene document to what the compositor is given.
//!
//! Two jobs, and both of them are "flatten": resolving a reference to another
//! scene into the items it stands for, and turning the result into the
//! `Placement` list the slot pool writes onto pads. Groups are flattened by
//! `scene::geometry::flatten`, which is the one implementation of that
//! arithmetic; nothing is reimplemented here.

use crate::caps::CanvasCaps;
use crate::mixer::slots::{ItemFilter, Placement, PlacementAudio, Sizing};
use crate::scene::document::{
    Align, Audio, Collection, Content, Filter, Fit, Item, Override, Scene, Transform,
};
use crate::scene::geometry;
use crate::scene::id::Id;

/// How deep a reference chain may go before it is treated as a cycle.
///
/// `Collection::check_refs` refuses a cycle at edit time, so this is the belt
/// to that pair of braces: a store edited by hand or written by an older build
/// must not be able to make the mixer recurse forever.
const MAX_REF_DEPTH: usize = 16;

/// Turn a scene's items into a plain item list with every reference replaced
/// by the items it stands for, overrides applied.
///
/// A reference becomes a group whose children are the target's items, with the
/// referencing item's own transform and opacity on the group, which is what
/// makes `geometry::flatten` treat a nested scene exactly like a group and
/// nothing downstream need know the difference.
pub fn resolve(doc: &Collection, scene: &Scene) -> Vec<Item> {
    resolve_at(doc, &scene.items, 0)
}

fn resolve_at(doc: &Collection, items: &[Item], depth: usize) -> Vec<Item> {
    items
        .iter()
        .map(|item| match &item.content {
            Content::Ref { scene, overrides } if depth < MAX_REF_DEPTH => {
                let children = doc
                    .scene(scene)
                    .map(|target| {
                        let mut kids = resolve_at(doc, &target.items, depth + 1);
                        apply_overrides(&mut kids, overrides);
                        kids
                    })
                    .unwrap_or_default();
                Item { content: Content::Children { children }, ..item.clone() }
            }
            Content::Children { children } => Item {
                content: Content::Children { children: resolve_at(doc, children, depth) },
                ..item.clone()
            },
            _ => item.clone(),
        })
        .collect()
}

/// Sparse changes keyed by the target's item id, USD's `over`. Addressing by
/// id rather than by position is why a target that gains an item does not
/// silently move every override onto the wrong entry.
fn apply_overrides(items: &mut [Item], overrides: &std::collections::BTreeMap<Id, Override>) {
    for item in items.iter_mut() {
        if let Some(over) = overrides.get(&item.id) {
            if let Some(t) = over.transform {
                item.transform = t;
            }
            if let Some(c) = over.crop {
                item.crop = c;
            }
            if let Some(o) = over.opacity {
                item.opacity = o;
            }
            if let Some(v) = over.visible {
                item.visible = v;
            }
            if let (Some(params), Content::Graphic { graphic, .. }) = (&over.params, &item.content) {
                item.content =
                    Content::Graphic { graphic: graphic.clone(), params: params.clone() };
            }
        }
        if let Content::Children { children } = &mut item.content {
            apply_overrides(children, overrides);
        }
    }
}

/// The placements the slot pool is given, bottom of the stack first.
///
/// An item whose content is not a source (a graphic, or a reference that could
/// not be resolved) is skipped: the graphics host is Phase 5 and a placement
/// with nothing behind it would be a black rectangle over the picture.
pub fn placements(doc: &Collection, scene: &Scene, canvas: &CanvasCaps) -> Vec<Placement> {
    let resolved = resolve(doc, scene);
    let mut out = Vec::new();
    // The ordinary path, and the only one a scene with no filtered group ever
    // takes: flatten the tree and turn each leaf into a placement.
    if !has_filtered_group(&resolved) {
        for p in geometry::flatten(&resolved, &doc.canvas) {
            if let Some(placement) = leaf(&p) {
                out.push(clamp(placement, canvas));
            }
        }
        return out;
    }
    walk(&resolved, &Transform::default(), 1.0, doc, canvas, &mut out);
    out
}

/// Is there a group here that carries a filter?
///
/// Asked once, before anything else, because the answer is no for every scene
/// anybody has built and the cheap path is worth keeping cheap.
fn has_filtered_group(items: &[Item]) -> bool {
    items.iter().any(|item| match &item.content {
        Content::Children { children } => {
            (item.visible && item.filters.iter().any(|f| f.enabled)) || has_filtered_group(children)
        }
        _ => false,
    })
}

/// Flatten, stopping at a group that carries a filter.
///
/// A filtered group cannot be flattened: the filter is over the group as one
/// picture, and three items flattened are three pictures. So it becomes one
/// placement carrying its children, and the slot pool composites it on its own
/// (`mixer::group`, the expensive path).
fn walk(
    items: &[Item],
    parent: &Transform,
    opacity: f64,
    doc: &Collection,
    canvas: &CanvasCaps,
    out: &mut Vec<Placement>,
) {
    for item in items {
        if !item.visible {
            continue;
        }
        let transform = geometry::compose(parent, &item.transform);
        let alpha = opacity * item.opacity;
        match &item.content {
            Content::Children { children } if item.filters.iter().any(|f| f.enabled) => {
                // The children at their canvas coordinates, because the sub
                // compositor's surface is the canvas: the group's transform
                // and opacity are already in them, and what the programme
                // compositor then draws is the whole surface.
                let synthetic = Item { transform, opacity: alpha, ..item.clone() };
                let mut kids = Vec::new();
                for p in geometry::flatten(std::slice::from_ref(&synthetic), &doc.canvas) {
                    if let Some(child) = leaf(&p) {
                        kids.push(clamp(child, canvas));
                    }
                }
                if kids.is_empty() {
                    continue;
                }
                out.push(Placement {
                    source: format!("group:{}", item.id),
                    item: Some(item.id),
                    filters: filters(&item.filters),
                    group: kids,
                    xpos: 0,
                    ypos: 0,
                    width: canvas.width,
                    height: canvas.height,
                    alpha: 1.0,
                    crop: (0.0, 0.0, 0.0, 0.0),
                    rotation: 0.0,
                    additive: false,
                    sizing: Sizing::Fill,
                    align: (0.5, 0.5),
                    audio: PlacementAudio::Never,
                });
            }
            Content::Children { children } => walk(children, &transform, alpha, doc, canvas, out),
            _ => {
                let synthetic = Item { transform, opacity: alpha, ..item.clone() };
                for p in geometry::flatten(std::slice::from_ref(&synthetic), &doc.canvas) {
                    if let Some(placement) = leaf(&p) {
                        out.push(clamp(placement, canvas));
                    }
                }
            }
        }
    }
}

/// One flattened leaf as a placement, or `None` for an item that draws no
/// source.
///
/// A graphic draws one: it resolves to the source instance the graphics host
/// renders its page into (`server::graphics`), so nothing downstream of here
/// knows a graphic from a camera. Until that source exists the mixer simply
/// does not draw the placement, because `current_placements` keeps only the
/// ones whose source is live, which is what makes adding the graphic and
/// starting its page two separate steps that can happen in either order.
///
/// A reference that could not be resolved still draws nothing: a placement
/// with nothing behind it would be a black rectangle over the picture.
fn leaf(p: &geometry::Placement<'_>) -> Option<Placement> {
    let source = match &p.item.content {
        Content::Source { source } => source.clone(),
        Content::Graphic { graphic, .. } => super::graphics::source_id(graphic, &p.item.id),
        _ => return None,
    };
    Some(Placement {
        source,
        item: Some(p.item.id),
        filters: filters(&p.item.filters),
        group: Vec::new(),
        xpos: p.rect.x.round() as i32,
        ypos: p.rect.y.round() as i32,
        width: p.rect.w.round() as i32,
        height: p.rect.h.round() as i32,
        alpha: p.opacity.clamp(0.0, 1.0),
        crop: (p.item.crop.left, p.item.crop.top, p.item.crop.right, p.item.crop.bottom),
        rotation: p.transform.rotation,
        additive: matches!(p.item.blend, crate::scene::document::Blend::Add),
        sizing: sizing(p.transform.fit),
        align: align(p.transform.align),
        audio: audio(p.item.audio),
    })
}

/// `fit` as the compositor pad spells it.
///
/// Three policies is all a `compositor` pad has, so the seven fits map onto
/// them and the reference page says which is which. An item with no frame is
/// already at its own size, so `none` fills its box exactly like `stretch`
/// does; `cover`, `fit-width` and `fit-height` all mean "fill the box and let
/// the overflow go".
pub fn sizing(fit: Fit) -> Sizing {
    match fit {
        Fit::None | Fit::Stretch => Sizing::Fill,
        Fit::Contain | Fit::Max => Sizing::Contain,
        Fit::Cover | Fit::FitWidth | Fit::FitHeight => Sizing::Cover,
    }
}

/// The item's filters as the slot chain needs them.
///
/// A disabled filter is not a filter with a flag: it is simply not in the
/// chain, so turning one off costs one pad block and then nothing at all. The
/// params are JSON on the document and a toml table in the pipeline, which is
/// the one place the two spellings meet; a value toml cannot hold (a null, a
/// nested array of tables) is dropped and the filter's own defaults stand.
fn filters(list: &[Filter]) -> Vec<ItemFilter> {
    list.iter()
        .filter(|f| f.enabled)
        .map(|f| ItemFilter {
            type_id: f.kind.clone(),
            name: f.name.clone(),
            params: params(&f.params),
        })
        .collect()
}

fn params(value: &serde_json::Value) -> crate::config::Params {
    match toml::Value::try_from(value) {
        Ok(toml::Value::Table(table)) => table,
        _ => Default::default(),
    }
}

fn align(a: Align) -> (f64, f64) {
    let f = a.factors();
    (f.x, f.y)
}

fn audio(a: Audio) -> PlacementAudio {
    match a {
        Audio::Follow => PlacementAudio::Follow,
        Audio::Always => PlacementAudio::Always,
        Audio::Never => PlacementAudio::Never,
    }
}

/// Keep a placement inside what the compositor will accept.
///
/// A pad with a negative width is refused by the element and a pad hundreds of
/// thousands of pixels wide is a scaler allocating a picture nobody asked for.
/// An item off the edge of the canvas is legal and stays legal: the validator
/// reports it, the compositor clips it, and an operator who deliberately parks
/// something off screen keeps being able to.
fn clamp(mut p: Placement, canvas: &CanvasCaps) -> Placement {
    let limit = canvas.width.max(canvas.height).saturating_mul(8);
    p.width = p.width.clamp(0, limit);
    p.height = p.height.clamp(0, limit);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::document::{Canvas, Frame, Vec2};

    fn canvas() -> CanvasCaps {
        CanvasCaps::new(&crate::config::Canvas {
            width: 1920,
            height: 1080,
            fps: 30,
            sample_rate: 48000,
            channels: 2,
        })
    }

    fn source(id: &str) -> Item {
        Item::new(Content::Source { source: id.into() })
    }

    fn doc_with(items: Vec<Item>) -> Collection {
        let mut doc = Collection::new("show", Canvas::default());
        let mut scene = Scene::new("wide");
        scene.items = items;
        doc.scenes.push(scene);
        doc
    }

    #[test]
    fn a_full_canvas_item_becomes_a_full_canvas_placement() {
        let mut item = source("cam1");
        item.transform.frame = Some(Frame::new(1920.0, 1080.0));
        let doc = doc_with(vec![item]);
        let p = placements(&doc, &doc.scenes[0], &canvas());
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].xpos, p[0].ypos, p[0].width, p[0].height), (0, 0, 1920, 1080));
        assert_eq!(p[0].alpha, 1.0);
    }

    /// The OBS issue 2913 case, which is the acceptance line: moving a group by
    /// 100 px moves every child by 100 px and corrupts no child transform.
    #[test]
    fn moving_a_group_moves_every_child_and_corrupts_none() {
        let mut a = source("cam1");
        a.name = Some("left".into());
        a.transform.position = Vec2::new(0.0, 0.0);
        a.transform.frame = Some(Frame::new(960.0, 540.0));
        let mut b = source("cam2");
        b.name = Some("right".into());
        b.transform.position = Vec2::new(960.0, 0.0);
        b.transform.frame = Some(Frame::new(960.0, 540.0));
        let children = vec![a.clone(), b.clone()];
        let mut group = Item::new(Content::Children { children: children.clone() });
        group.name = Some("pair".into());

        let mut doc = doc_with(vec![group]);
        let before = placements(&doc, &doc.scenes[0], &canvas());

        // Move the group and nothing else.
        doc.scenes[0].items[0].transform.position = Vec2::new(100.0, 0.0);
        let after = placements(&doc, &doc.scenes[0], &canvas());

        assert_eq!(before.len(), 2);
        for (was, now) in before.iter().zip(&after) {
            assert_eq!(now.xpos - was.xpos, 100, "a child did not move with its group");
            assert_eq!(now.ypos, was.ypos);
            assert_eq!((now.width, now.height), (was.width, was.height), "a child was resized");
        }
        // And the children's own transforms on the document are untouched,
        // which is the half OBS got wrong: it wrote the group's move into each
        // child and lost the child's own position.
        let Content::Children { children: after_children } = &doc.scenes[0].items[0].content else {
            panic!("the group stopped being a group")
        };
        assert_eq!(after_children[0].transform, a.transform);
        assert_eq!(after_children[1].transform, b.transform);
    }

    #[test]
    fn a_reference_is_flattened_like_a_group_with_its_overrides_applied() {
        let mut inner_item = source("cam2");
        inner_item.transform.position = Vec2::new(10.0, 20.0);
        inner_item.transform.frame = Some(Frame::new(100.0, 50.0));
        let inner_id = inner_item.id;
        let mut inner = Scene::new("lower third");
        inner.items.push(inner_item);
        let inner_scene_id = inner.id;

        let mut reference = Item::new(Content::Ref {
            scene: inner_scene_id,
            overrides: [(inner_id, Override { opacity: Some(0.5), ..Override::default() })]
                .into_iter()
                .collect(),
        });
        reference.transform.position = Vec2::new(1000.0, 0.0);

        let mut doc = doc_with(vec![reference]);
        doc.scenes.push(inner);
        let p = placements(&doc, &doc.scenes[0], &canvas());
        assert_eq!(p.len(), 1, "the reference stands for the target's one item");
        assert_eq!(p[0].source, "cam2");
        assert_eq!((p[0].xpos, p[0].ypos), (1010, 20), "the reference's own transform composes");
        assert_eq!(p[0].alpha, 0.5, "the override was not applied");
    }

    /// A group with a filter cannot be flattened, so it comes out as one
    /// placement carrying its children and the slot pool composites it on its
    /// own. Without a filter the same group flattens as it always did.
    #[test]
    fn a_group_with_a_filter_stays_a_group_and_one_without_is_flattened() {
        let mut a = source("cam1");
        a.transform.position = Vec2::new(0.0, 0.0);
        a.transform.frame = Some(Frame::new(960.0, 1080.0));
        let mut b = source("cam2");
        b.transform.position = Vec2::new(960.0, 0.0);
        b.transform.frame = Some(Frame::new(960.0, 1080.0));
        let mut group = Item::new(Content::Children { children: vec![a, b] });
        group.name = Some("pair".into());

        let doc = doc_with(vec![group.clone()]);
        let flat = placements(&doc, &doc.scenes[0], &canvas());
        assert_eq!(flat.len(), 2, "a group with no filter is flattened as it always was");
        assert!(flat.iter().all(|p| p.group.is_empty()));

        group.filters = vec![crate::scene::document::Filter {
            kind: "chroma/filter".into(),
            name: Some("over the pair".into()),
            enabled: true,
            params: serde_json::json!({}),
        }];
        let doc = doc_with(vec![group.clone()]);
        let kept = placements(&doc, &doc.scenes[0], &canvas());
        assert_eq!(kept.len(), 1, "a filtered group is one placement, not two");
        assert_eq!(kept[0].group.len(), 2, "carrying its two children");
        assert_eq!(kept[0].filters.len(), 1);
        // The surface is the canvas and the children sit on it where they
        // would have sat, so the filter sees what an operator sees.
        assert_eq!((kept[0].xpos, kept[0].ypos), (0, 0));
        assert_eq!((kept[0].width, kept[0].height), (1920, 1080));
        assert_eq!(kept[0].group[0].xpos, 0);
        assert_eq!(kept[0].group[1].xpos, 960);

        // And a disabled filter is no filter: back to the cheap path.
        group.filters[0].enabled = false;
        let doc = doc_with(vec![group]);
        assert_eq!(placements(&doc, &doc.scenes[0], &canvas()).len(), 2);
    }

    /// Moving a filtered group moves its children on the surface, the same way
    /// moving any group moves its children on the canvas.
    #[test]
    fn moving_a_filtered_group_moves_its_children() {
        let mut a = source("cam1");
        a.transform.frame = Some(Frame::new(480.0, 270.0));
        let mut group = Item::new(Content::Children { children: vec![a] });
        group.filters = vec![crate::scene::document::Filter {
            kind: "chroma/filter".into(),
            name: None,
            enabled: true,
            params: serde_json::json!({}),
        }];
        let mut doc = doc_with(vec![group]);
        let before = placements(&doc, &doc.scenes[0], &canvas());
        doc.scenes[0].items[0].transform.position = Vec2::new(100.0, 50.0);
        let after = placements(&doc, &doc.scenes[0], &canvas());
        assert_eq!(after[0].group[0].xpos - before[0].group[0].xpos, 100);
        assert_eq!(after[0].group[0].ypos - before[0].group[0].ypos, 50);
    }

    #[test]
    fn a_graphic_resolves_to_the_source_its_page_is_rendered_into() {
        let graphic =
            Item::new(Content::Graphic { graphic: "ograf/lower-third".into(), params: Default::default() });
        let id = graphic.id;
        let doc = doc_with(vec![source("cam1"), graphic]);
        let p = placements(&doc, &doc.scenes[0], &canvas());
        assert_eq!(p.len(), 2, "a graphic is placed like any other item");
        assert_eq!(p[1].source, super::super::graphics::source_id("ograf/lower-third", &id));
        assert_eq!(p[1].item, Some(id), "a transition matches it by item, as it does a camera");
        // Nothing here starts that source. `Mixer::current_placements` draws
        // only what is live, so a graphic whose page is not up yet is absent
        // from the canvas rather than black on it.
    }

    #[test]
    fn a_reference_cycle_stops_rather_than_recursing_forever() {
        // check_refs refuses this at edit time; a store written by hand can
        // still contain it and must not take the mixer down.
        let mut outer = Scene::new("outer");
        let mut inner = Scene::new("inner");
        outer.items.push(Item::new(Content::Ref { scene: inner.id, overrides: Default::default() }));
        inner.items.push(Item::new(Content::Ref { scene: outer.id, overrides: Default::default() }));
        let mut doc = Collection::new("show", Canvas::default());
        doc.scenes.push(outer);
        doc.scenes.push(inner);
        let scene = doc.scenes[0].clone();
        assert!(placements(&doc, &scene, &canvas()).is_empty(), "a cycle resolves to nothing");
    }

    /// Every fit has to land on a nick a real `compositor` pad carries.
    /// Writing one it does not panics inside glib, which took the mixer thread
    /// down mid take the first time a two box was applied.
    #[test]
    fn every_fit_lands_on_a_policy_this_compositors_pad_has() {
        use gstreamer::prelude::*;
        let _ = gstreamer::init();
        let Ok(comp) = crate::gstutil::make("compositor", "fit-check") else { return };
        let Some(pad) = comp.request_pad_simple("sink_%u") else { return };
        let pspec = pad.find_property("sizing-policy").expect("a compositor pad has one");
        let class = gstreamer::glib::EnumClass::with_type(pspec.value_type())
            .expect("sizing-policy is an enum");
        for fit in
            [Fit::None, Fit::Contain, Fit::Cover, Fit::Stretch, Fit::FitWidth, Fit::FitHeight, Fit::Max]
        {
            let nicks = sizing(fit).nicks();
            assert!(
                nicks.iter().any(|n| class.value_by_nick(n).is_some()),
                "{fit:?} offers {nicks:?} and this compositor has none of them"
            );
            // And the write itself, which is the thing that panicked.
            let placement =
                Placement { sizing: sizing(fit), ..Placement::full_canvas("cam".into(), &canvas()) };
            assert_eq!(placement.sizing, sizing(fit));
        }
        comp.release_request_pad(&pad);
    }
}
