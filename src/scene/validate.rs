//! `scene.validate`: what is wrong with a scene, as data.
//!
//! The designer draws these as badges, the CLI prints them as lines, an agent
//! reads them as JSON. None of them stops a document being saved or taken to
//! air: a deliberate off canvas item is a legitimate thing to build, and a
//! validator that refuses one is a validator people turn off.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::document::{Canvas, Collection, Scene};
use super::geometry::{flatten, Rect};
use super::id::Id;

/// How much the reader should care.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The scene will not composite as written.
    Error,
    /// It will composite, and it is probably not what was meant.
    Warning,
    /// Worth knowing before going to air.
    Info,
}

/// One thing the validator found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    pub severity: Severity,
    /// A stable machine readable code, so a client can filter or translate.
    pub code: String,
    /// The scene it is in, when it is about one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<Id>,
    /// The items involved, in the order the message names them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Id>,
    /// One sentence naming the state and the next step.
    pub message: String,
    /// The numbers behind the message, for a client that draws them.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub detail: serde_json::Value,
}

/// The margins broadcast has used since the cathode ray tube, kept because
/// phone crops, stream overlays and TV overscan still eat the same edges.
/// Anything outside the action safe box may be cut off; anything outside the
/// title safe box should carry no text.
pub const ACTION_SAFE: f64 = 0.07;
pub const TITLE_SAFE: f64 = 0.10;

/// Check a whole collection.
pub fn collection(doc: &Collection) -> Vec<Finding> {
    let mut out = Vec::new();
    if let Err(e) = doc.check_refs() {
        out.push(Finding {
            severity: Severity::Error,
            code: "scene.ref".into(),
            scene: None,
            items: Vec::new(),
            message: e.to_string(),
            detail: serde_json::Value::Null,
        });
    }
    for scene in &doc.scenes {
        out.extend(self::scene(scene, &doc.canvas));
    }
    out.extend(duplicate_ids(doc));
    out
}

/// Check one scene against a canvas.
pub fn scene(scene: &Scene, canvas: &Canvas) -> Vec<Finding> {
    let mut out = Vec::new();
    let placed = flatten(&scene.items, canvas);
    let canvas_rect = Rect::of(canvas);
    let action = canvas_rect.inset_fraction(ACTION_SAFE);
    let title = canvas_rect.inset_fraction(TITLE_SAFE);

    for p in &placed {
        if p.rect.intersect(&canvas_rect).is_none() {
            out.push(off_canvas(scene, p, "scene.off_canvas", Severity::Warning, "is entirely off the canvas"));
        } else if !p.rect.inside(&canvas_rect) && !canvas_rect.inside(&p.rect) {
            out.push(off_canvas(scene, p, "scene.off_canvas_partly", Severity::Info, "hangs over the edge of the canvas"));
        }
        // A background is meant to run to the edges. Anything that covers the
        // whole safe area is one, so only items inside the canvas and smaller
        // than the safe box can breach it.
        if p.rect.inside(&canvas_rect) && !p.rect.inside(&title) && !title.inside(&p.rect) {
            let (code, severity, box_name) = if p.rect.inside(&action) {
                ("scene.title_safe", Severity::Info, "title safe")
            } else {
                ("scene.action_safe", Severity::Warning, "action safe")
            };
            let (x, y, w, h) = p.rect.rounded();
            out.push(Finding {
                severity,
                code: code.into(),
                scene: Some(scene.id),
                items: vec![p.item.id],
                message: format!(
                    "{} at {x},{y} {w}x{h} reaches outside the {box_name} area. Move it in, or accept that a phone crop or an overscanning TV may cut it.",
                    p.path
                ),
                detail: json!({ "rect": [x, y, w, h], "safe": rect_json(&title), "action_safe": rect_json(&action) }),
            });
        }
    }
    out.extend(overlaps(scene, &placed, &canvas_rect));
    out
}

/// An item off the canvas, entirely or partly.
fn off_canvas(
    scene: &Scene,
    p: &super::geometry::Placement<'_>,
    code: &str,
    severity: Severity,
    what: &str,
) -> Finding {
    let (x, y, w, h) = p.rect.rounded();
    Finding {
        severity,
        code: code.into(),
        scene: Some(scene.id),
        items: vec![p.item.id],
        message: format!(
            "{} at {x},{y} {w}x{h} {what}. Set its position, or hide it if it is parked deliberately.",
            p.path
        ),
        detail: json!({ "rect": [x, y, w, h] }),
    }
}

/// Items that cover each other.
///
/// Two items overlapping is ordinary: that is what a picture in picture is. The
/// report is for the one that hides another completely, because that is nearly
/// always a mistake or a leftover, and the operator cannot see it to select it.
fn overlaps(
    scene: &Scene,
    placed: &[super::geometry::Placement<'_>],
    canvas: &Rect,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, under) in placed.iter().enumerate() {
        for over in placed.iter().skip(i + 1) {
            // Only an opaque item in normal blend actually hides anything.
            if over.opacity < 1.0 || over.item.blend != super::document::Blend::Normal {
                continue;
            }
            let Some(shared) = under.rect.intersect(&over.rect) else { continue };
            let visible = under.rect.intersect(canvas).map(|r| r.area()).unwrap_or(0.0);
            if visible <= 0.0 || shared.area() + 1.0 < visible {
                continue;
            }
            let (x, y, w, h) = under.rect.rounded();
            out.push(Finding {
                severity: Severity::Warning,
                code: "scene.hidden".into(),
                scene: Some(scene.id),
                items: vec![under.item.id, over.item.id],
                message: format!(
                    "{} at {x},{y} {w}x{h} is completely covered by {}. Reorder them, shrink the one on top, or remove the one underneath.",
                    under.path, over.path
                ),
                detail: json!({ "under": rect_json(&under.rect), "over": rect_json(&over.rect) }),
            });
        }
    }
    out
}

/// Two nodes with the same id. It cannot happen through the commands, and it
/// happens all the time in a file somebody edited by hand or copied.
fn duplicate_ids(doc: &Collection) -> Vec<Finding> {
    let mut seen: std::collections::BTreeMap<Id, usize> = std::collections::BTreeMap::new();
    for scene in &doc.scenes {
        *seen.entry(scene.id).or_default() += 1;
        for item in scene.walk() {
            *seen.entry(item.id).or_default() += 1;
        }
    }
    seen.into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(id, n)| Finding {
            severity: Severity::Error,
            code: "scene.duplicate_id".into(),
            scene: None,
            items: vec![id],
            message: format!(
                "id {id} is used by {n} nodes. Every node carries its own id and never reuses one; give the copies new ids."
            ),
            detail: json!({ "count": n }),
        })
        .collect()
}

/// A rectangle as four numbers, for the `detail` block.
fn rect_json(r: &Rect) -> serde_json::Value {
    let (x, y, w, h) = r.rounded();
    json!([x, y, w, h])
}

/// A one line summary for a command line, or `None` when nothing was found.
pub fn summary(findings: &[Finding]) -> Option<String> {
    if findings.is_empty() {
        return None;
    }
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    Some(format!(
        "{} error(s), {} warning(s), {} note(s)",
        count(Severity::Error),
        count(Severity::Warning),
        count(Severity::Info)
    ))
}

/// True when anything found would stop the scene compositing as written.
pub fn has_errors(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.severity == Severity::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::document::*;
    use crate::scene::geometry::Rect;

    fn item_at(x: f64, y: f64, w: f64, h: f64) -> Item {
        let mut item = Item::new(Content::Source { source: "cam".into() });
        item.transform.position = Vec2::new(x, y);
        item.transform.frame = Some(Frame::new(w, h));
        item
    }

    fn scene_of(items: Vec<Item>) -> Scene {
        let mut s = Scene::new("test");
        s.items = items;
        s
    }

    #[test]
    fn a_clean_scene_reports_nothing() {
        let mut background = item_at(0.0, 0.0, 1920.0, 1080.0);
        background.name = Some("bg".into());
        let mut inset = item_at(1300.0, 700.0, 480.0, 270.0);
        inset.name = Some("inset".into());
        let found = scene(&scene_of(vec![background, inset]), &Canvas::default());
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn an_item_parked_off_the_canvas_is_reported_with_its_rectangle() {
        let mut off = item_at(3000.0, 0.0, 200.0, 200.0);
        off.name = Some("parked".into());
        let found = scene(&scene_of(vec![off]), &Canvas::default());
        assert_eq!(found[0].code, "scene.off_canvas");
        assert!(found[0].message.contains("parked"), "{}", found[0].message);
        assert_eq!(found[0].detail["rect"], json!([3000, 0, 200, 200]));
    }

    #[test]
    fn an_item_hanging_over_the_edge_is_a_note_not_a_warning() {
        let found = scene(&scene_of(vec![item_at(1800.0, 0.0, 400.0, 200.0)]), &Canvas::default());
        let edge = found.iter().find(|f| f.code == "scene.off_canvas_partly").expect("a note");
        assert_eq!(edge.severity, Severity::Info);
    }

    #[test]
    fn an_item_completely_under_another_is_reported_as_hidden() {
        let mut under = item_at(100.0, 100.0, 200.0, 200.0);
        under.name = Some("forgotten".into());
        let mut over = item_at(0.0, 0.0, 1920.0, 1080.0);
        over.name = Some("background".into());
        let found = scene(&scene_of(vec![under, over]), &Canvas::default());
        let hidden = found.iter().find(|f| f.code == "scene.hidden").expect("a hidden finding");
        assert!(hidden.message.contains("forgotten"), "{}", hidden.message);
        assert!(hidden.message.contains("background"), "{}", hidden.message);
        assert_eq!(hidden.items.len(), 2);
    }

    #[test]
    fn a_translucent_item_on_top_hides_nothing() {
        let under = item_at(100.0, 100.0, 200.0, 200.0);
        let mut over = item_at(0.0, 0.0, 1920.0, 1080.0);
        over.opacity = 0.5;
        let found = scene(&scene_of(vec![under, over]), &Canvas::default());
        assert!(!found.iter().any(|f| f.code == "scene.hidden"), "{found:#?}");
    }

    #[test]
    fn text_near_the_edge_breaches_the_safe_areas_in_order() {
        // Inside the canvas, outside title safe, inside action safe.
        let canvas = Canvas::default();
        let title = Rect::of(&canvas).inset_fraction(TITLE_SAFE);
        let mut near = item_at(title.x - 20.0, 500.0, 100.0, 40.0);
        near.name = Some("ticker".into());
        let found = scene(&scene_of(vec![near]), &canvas);
        assert_eq!(found[0].code, "scene.title_safe");
        assert_eq!(found[0].severity, Severity::Info);

        let mut edge = item_at(4.0, 500.0, 100.0, 40.0);
        edge.name = Some("ticker".into());
        let found = scene(&scene_of(vec![edge]), &canvas);
        assert_eq!(found[0].code, "scene.action_safe");
        assert_eq!(found[0].severity, Severity::Warning);
    }

    #[test]
    fn a_full_canvas_background_does_not_breach_a_safe_area() {
        let found = scene(&scene_of(vec![item_at(0.0, 0.0, 1920.0, 1080.0)]), &Canvas::default());
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn two_nodes_sharing_an_id_is_an_error_with_the_count() {
        let mut doc = Collection::new("dup", Canvas::default());
        let a = item_at(0.0, 0.0, 100.0, 100.0);
        let mut b = item_at(0.0, 0.0, 100.0, 100.0);
        b.id = a.id;
        doc.scenes.push(scene_of(vec![a, b]));
        let found = collection(&doc);
        let dup = found.iter().find(|f| f.code == "scene.duplicate_id").expect("a duplicate");
        assert_eq!(dup.severity, Severity::Error);
        assert!(has_errors(&found));
        assert!(summary(&found).unwrap().contains("error"));
    }

    #[test]
    fn a_group_is_validated_through_its_children_not_as_a_box() {
        let mut child = item_at(3000.0, 0.0, 100.0, 100.0);
        child.name = Some("child".into());
        let mut group = Item::new(Content::Children { children: vec![child] });
        group.name = Some("group".into());
        let found = scene(&scene_of(vec![group]), &Canvas::default());
        assert_eq!(found.len(), 1);
        assert!(found[0].message.starts_with("group / child"), "{}", found[0].message);
    }
}
