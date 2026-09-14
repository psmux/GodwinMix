//! From a scene document to what the compositor is given.
//!
//! Two jobs, and both of them are "flatten": resolving a reference to another
//! scene into the items it stands for, and turning the result into the
//! `Placement` list the slot pool writes onto pads. Groups are flattened by
//! `scene::geometry::flatten`, which is the one implementation of that
//! arithmetic; nothing is reimplemented here.

use crate::caps::CanvasCaps;
use crate::mixer::slots::{Placement, PlacementAudio};
use crate::scene::document::{
    Align, Audio, Collection, Content, Fit, Item, Override, Scene,
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
    geometry::flatten(&resolved, &doc.canvas)
        .into_iter()
        .filter_map(|p| {
            let Content::Source { source } = &p.item.content else { return None };
            Some(Placement {
                source: source.clone(),
                xpos: p.rect.x.round() as i32,
                ypos: p.rect.y.round() as i32,
                width: p.rect.w.round() as i32,
                height: p.rect.h.round() as i32,
                alpha: p.opacity.clamp(0.0, 1.0),
                crop: (p.item.crop.left, p.item.crop.top, p.item.crop.right, p.item.crop.bottom),
                rotation: p.transform.rotation,
                sizing: sizing(p.transform.fit),
                align: align(p.transform.align),
                audio: audio(p.item.audio),
            })
        })
        .map(|p| clamp(p, canvas))
        .collect()
}

/// `fit` as the compositor pad spells it.
///
/// Three policies is all a `compositor` pad has, so the seven fits map onto
/// them and the reference page says which is which. `cover`, `fit-width` and
/// `fit-height` all mean "fill the box and let the overflow go", which on this
/// element is what `scale` plus the item's own crop gives.
pub fn sizing(fit: Fit) -> &'static str {
    match fit {
        Fit::None => "none",
        Fit::Contain | Fit::Max => "keep-aspect-ratio",
        Fit::Stretch | Fit::Cover | Fit::FitWidth | Fit::FitHeight => "scale",
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

    #[test]
    fn a_graphic_is_skipped_rather_than_drawn_as_a_black_box() {
        let doc = doc_with(vec![
            source("cam1"),
            Item::new(Content::Graphic { graphic: "ograf/lower-third".into(), params: Default::default() }),
        ]);
        let p = placements(&doc, &doc.scenes[0], &canvas());
        assert_eq!(p.len(), 1, "a graphic with no host must not become a rectangle");
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

    #[test]
    fn the_seven_fits_land_on_a_policy_the_compositor_has() {
        for fit in [Fit::None, Fit::Contain, Fit::Cover, Fit::Stretch, Fit::FitWidth, Fit::FitHeight, Fit::Max] {
            assert!(
                ["none", "keep-aspect-ratio", "scale"].contains(&sizing(fit)),
                "{fit:?} maps onto a policy the element does not have"
            );
        }
    }
}
