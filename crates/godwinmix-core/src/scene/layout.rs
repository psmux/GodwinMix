//! Layout presets: a scene whose sources are parameters.
//!
//! "Put the guest in the bottom right corner" is the same scene every time
//! except for which camera goes where and how big the inset is, so it is a
//! document with a `params` block and a binding per geometry field (11 section
//! 6a). Applying one resolves the params and produces an ordinary scene.
//!
//! The item ids are derived from the scene's id and the layout item, so
//! applying a layout twice lands on the same items. That is what makes
//! "grow the inset to full screen" a property ramp with a duration rather than
//! a cut: the item that was the inset is the item that becomes full screen.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use super::document::*;
use super::expr::{self, Scope};
use super::id::Id;

/// Every layout that ships with the core, in the order `gmx scene layout
/// --list` prints them.
pub const NAMES: &[&str] = &[
    "full",
    "pip-top-left",
    "pip-top-right",
    "pip-bottom-left",
    "pip-bottom-right",
    "two-box",
    "three-box",
    "quad",
    "l-shape",
    "split",
    "multiview",
];

/// The layout documents, compiled in so a core with no repository beside it
/// still has them. The files under `layouts/` are the source of truth; a plugin
/// or a person copies one and changes it rather than writing from nothing.
const FILES: &[(&str, &str)] = &[
    ("full", include_str!("../../../../layouts/full.json")),
    (
        "pip-top-left",
        include_str!("../../../../layouts/pip-top-left.json"),
    ),
    (
        "pip-top-right",
        include_str!("../../../../layouts/pip-top-right.json"),
    ),
    (
        "pip-bottom-left",
        include_str!("../../../../layouts/pip-bottom-left.json"),
    ),
    (
        "pip-bottom-right",
        include_str!("../../../../layouts/pip-bottom-right.json"),
    ),
    ("two-box", include_str!("../../../../layouts/two-box.json")),
    (
        "three-box",
        include_str!("../../../../layouts/three-box.json"),
    ),
    ("quad", include_str!("../../../../layouts/quad.json")),
    ("l-shape", include_str!("../../../../layouts/l-shape.json")),
    ("split", include_str!("../../../../layouts/split.json")),
    (
        "multiview",
        include_str!("../../../../layouts/multiview.json"),
    ),
];

/// The built in layout by name.
pub fn builtin(name: &str) -> Result<Collection> {
    let (_, text) = FILES.iter().find(|(n, _)| *n == name).with_context(|| {
        format!(
            "there is no built in layout called {name:?}. The built in layouts are: {}",
            NAMES.join(", ")
        )
    })?;
    Collection::from_json(text).with_context(|| format!("reading the built in layout {name:?}"))
}

/// The values an operator or a client supplies for a layout's params.
pub type Values = BTreeMap<String, Value>;

/// Read the `a=cam1,b=cam2,inset=0.4` form a command line takes.
pub fn parse_values(text: &str) -> Result<Values> {
    let mut out = Values::new();
    for pair in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').with_context(|| {
            format!("{pair:?} is not a value. Write them as name=value, separated by commas, for example a=cam1,b=cam2,inset=0.4")
        })?;
        let value = value.trim();
        let parsed = match value.parse::<f64>() {
            Ok(n) if !value.is_empty() => Value::from(n),
            _ => Value::from(value.to_string()),
        };
        out.insert(key.trim().to_string(), parsed);
    }
    Ok(out)
}

/// Apply a layout, producing a new scene with a fresh id.
pub fn apply(layout: &Collection, values: &Values, canvas: Canvas) -> Result<Scene> {
    apply_into(layout, values, canvas, Id::new(), None)
}

/// Apply a layout onto a scene that already exists, keeping its id, so the
/// items land on the items that are already there.
pub fn apply_into(
    layout: &Collection,
    values: &Values,
    canvas: Canvas,
    scene_id: Id,
    name: Option<&str>,
) -> Result<Scene> {
    let source = layout
        .scenes
        .first()
        .with_context(|| format!("the layout {:?} has no scene in it", layout.name))?;
    let resolved = resolve_values(layout, values)?;
    let scope = numeric_scope(&resolved, canvas);
    let ratio = ratio(layout.canvas, canvas);
    let items = resolve_items(&source.items, &resolved, &scope, ratio, scene_id)?;
    Ok(Scene {
        id: scene_id,
        name: name.unwrap_or(&source.name).to_string(),
        items,
        color: source.color.clone(),
    })
}

/// How much to stretch any geometry the layout wrote as a literal, from the
/// canvas the layout was authored at to the one being applied.
fn ratio(from: Canvas, to: Canvas) -> (f64, f64) {
    (
        to.width as f64 / from.width as f64,
        to.height as f64 / from.height as f64,
    )
}

/// Fill in the defaults from the params schema and refuse anything the layout
/// does not declare, because a silently ignored value is a layout that looks
/// broken for no visible reason.
fn resolve_values(layout: &Collection, given: &Values) -> Result<Values> {
    let properties = layout.params.get("properties").and_then(Value::as_object);
    let known: Vec<&String> = properties.map(|p| p.keys().collect()).unwrap_or_default();
    for key in given.keys() {
        if !known.contains(&key) {
            let names: Vec<&str> = known.iter().map(|k| k.as_str()).collect();
            bail!(
                "the layout {:?} has no parameter called {key:?}. It takes: {}",
                layout.name,
                if names.is_empty() {
                    "nothing".to_string()
                } else {
                    names.join(", ")
                }
            );
        }
    }
    let mut out = Values::new();
    for (key, schema) in properties.into_iter().flatten() {
        let value = given
            .get(key)
            .cloned()
            .or_else(|| schema.get("default").cloned())
            .unwrap_or_else(|| match schema.get("type").and_then(Value::as_str) {
                Some("number") | Some("integer") => Value::from(0.0),
                _ => Value::from(""),
            });
        out.insert(key.clone(), value);
    }
    for key in layout
        .params
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(key) = key.as_str() else { continue };
        let missing = out.get(key).is_none_or(|v| v.as_str() == Some(""));
        if missing {
            bail!(
                "the layout {:?} needs a value for {key:?}. Pass it as --values {key}=<source id>.",
                layout.name
            );
        }
    }
    Ok(out)
}

/// The names a binding may use: every numeric param, plus the canvas.
fn numeric_scope(values: &Values, canvas: Canvas) -> Scope {
    let mut scope = Scope::new();
    for (key, value) in values {
        if let Some(n) = value.as_f64() {
            scope.insert(key.clone(), n);
        }
    }
    scope.insert("W".into(), canvas.width as f64);
    scope.insert("H".into(), canvas.height as f64);
    scope
}

/// Resolve one level of items, then their children.
fn resolve_items(
    items: &[Item],
    values: &Values,
    scope: &Scope,
    ratio: (f64, f64),
    scene: Id,
) -> Result<Vec<Item>> {
    let mut out = Vec::new();
    for item in items {
        let mut next = item.clone();
        next.id = Id::derive(&scene, &item.id.to_string());
        next.name = item.name.as_deref().map(|n| substitute_text(n, values));
        next.content = match &item.content {
            Content::Source { source } => {
                let source = substitute_text(source, values);
                // A layout slot nobody filled in is not an item. This is what
                // lets `quad` be used with three cameras.
                if source.is_empty() {
                    continue;
                }
                Content::Source { source }
            }
            Content::Graphic { graphic, params } => {
                let graphic = substitute_text(graphic, values);
                if graphic.is_empty() {
                    continue;
                }
                Content::Graphic {
                    graphic,
                    params: substitute(params, values),
                }
            }
            Content::Ref {
                scene: target,
                overrides,
            } => Content::Ref {
                scene: *target,
                overrides: overrides.clone(),
            },
            Content::Children { children } => Content::Children {
                children: resolve_items(children, values, scope, ratio, scene)?,
            },
        };
        for filter in &mut next.filters {
            filter.params = substitute(&filter.params, values);
        }
        scale_literals(&mut next.transform, ratio);
        bind(&mut next, scope)?;
        next.bind.clear();
        out.push(next);
    }
    Ok(out)
}

/// Stretch the geometry a layout wrote as a literal onto the target canvas.
/// A bound field is overwritten straight after, so this only touches what the
/// layout author typed in pixels.
fn scale_literals(transform: &mut Transform, (rx, ry): (f64, f64)) {
    transform.position = Vec2::new(transform.position.x * rx, transform.position.y * ry);
    transform.frame = transform.frame.map(|f| Frame::new(f.w * rx, f.h * ry));
}

/// Evaluate the item's bindings and write the results into its transform.
fn bind(item: &mut Item, scope: &Scope) -> Result<()> {
    let label = item.name.clone().unwrap_or_else(|| item.id.to_string());
    for (path, source) in &item.bind {
        let value =
            expr::eval(source, scope).map_err(|e| anyhow::anyhow!("item {label:?}: {e}"))?;
        let t = &mut item.transform;
        match path.as_str() {
            "position.x" => t.position.x = value,
            "position.y" => t.position.y = value,
            "frame.w" => t.frame.get_or_insert(Frame::new(0.0, 0.0)).w = value,
            "frame.h" => t.frame.get_or_insert(Frame::new(0.0, 0.0)).h = value,
            "scale.x" => t.scale.x = value,
            "scale.y" => t.scale.y = value,
            "anchor.x" => t.anchor.x = value,
            "anchor.y" => t.anchor.y = value,
            "rotation" => t.rotation = value,
            "opacity" => item.opacity = value,
            "crop.left" => item.crop.left = value,
            "crop.top" => item.crop.top = value,
            "crop.right" => item.crop.right = value,
            "crop.bottom" => item.crop.bottom = value,
            other => bail!(
                "item {label:?} binds {other:?}, which is not a field a layout can set. The fields are: position.x, position.y, frame.w, frame.h, scale.x, scale.y, anchor.x, anchor.y, rotation, opacity, crop.left, crop.top, crop.right, crop.bottom."
            ),
        }
    }
    Ok(())
}

/// Replace `{{name}}` in a string. A string that is nothing but one binding
/// takes the value's own text, so `{{a}}` with a source id gives the id.
fn substitute_text(text: &str, values: &Values) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start..].find("}}") else {
            break;
        };
        out.push_str(&rest[..start]);
        let key = rest[start + 2..start + end].trim();
        match values.get(key) {
            Some(Value::String(s)) => out.push_str(s),
            Some(other) => out.push_str(&other.to_string()),
            // An unfilled binding is left as it stands, so a half applied
            // layout shows what is missing instead of a blank.
            None => out.push_str(&rest[start..start + end + 2]),
        }
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

/// Substitute through a whole JSON value, strings only.
fn substitute(value: &Value, values: &Values) -> Value {
    match value {
        Value::String(s) => {
            // A whole string that is one binding takes the value's own type, so
            // a number stays a number.
            let trimmed = s.trim();
            if let (Some(key), true) = (trimmed.strip_prefix("{{"), trimmed.ends_with("}}")) {
                let key = key.trim_end_matches("}}").trim();
                if let Some(found) = values.get(key) {
                    return found.clone();
                }
            }
            Value::String(substitute_text(s, values))
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| substitute(v, values)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), substitute(v, values)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::geometry::{flatten, Rect, EPSILON};
    use crate::scene::validate;

    /// Every source slot of a layout filled with a distinct camera.
    fn all_sources(layout: &Collection) -> Values {
        let mut values = Values::new();
        let properties = layout.params.get("properties").and_then(Value::as_object);
        for (i, (key, schema)) in properties.into_iter().flatten().enumerate() {
            if schema.get("x-gmx-kind").and_then(Value::as_str) == Some("source") {
                values.insert(key.clone(), Value::from(format!("cam{}", i + 1)));
            }
            if schema.get("x-gmx-kind").and_then(Value::as_str) == Some("graphic") {
                values.insert(key.clone(), Value::from("lowerthird/graphic"));
            }
        }
        values
    }

    #[test]
    fn every_named_layout_is_a_file_and_every_file_is_named() {
        let files: Vec<&str> = FILES.iter().map(|(n, _)| *n).collect();
        assert_eq!(files, NAMES);
        for name in NAMES {
            let doc = builtin(name).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            assert_eq!(
                doc.scenes.len(),
                1,
                "a layout is one scene, {name} has {}",
                doc.scenes.len()
            );
            assert!(!doc.scenes[0].items.is_empty(), "{name} has no items");
            assert_eq!(doc.params["type"], "object", "{name} has no params block");
        }
    }

    #[test]
    fn every_layout_applies_at_both_canvas_sizes_with_every_item_on_the_canvas() {
        for name in NAMES {
            let layout = builtin(name).unwrap();
            let values = all_sources(&layout);
            for canvas in [
                Canvas {
                    width: 1920,
                    height: 1080,
                    fps: 30,
                },
                Canvas {
                    width: 1280,
                    height: 720,
                    fps: 30,
                },
            ] {
                let scene = apply(&layout, &values, canvas).unwrap_or_else(|e| {
                    panic!("{name} at {}x{}: {e:#}", canvas.width, canvas.height)
                });
                let placed = flatten(&scene.items, &canvas);
                assert!(!placed.is_empty(), "{name} produced no items");
                let area = Rect::of(&canvas);
                for p in &placed {
                    assert!(
                        p.rect.inside(&area),
                        "{name} at {}x{}: {} is at {:?}, off the canvas",
                        canvas.width,
                        canvas.height,
                        p.path,
                        p.rect.rounded()
                    );
                    assert!(
                        p.rect.w > 1.0 && p.rect.h > 1.0,
                        "{name}: {} has no size",
                        p.path
                    );
                }
            }
        }
    }

    #[test]
    fn no_two_items_of_a_layout_cover_each_other_except_the_pip_inside_its_main() {
        for name in NAMES {
            let layout = builtin(name).unwrap();
            let canvas = Canvas::default();
            let scene = apply(&layout, &all_sources(&layout), canvas).unwrap();
            let placed = flatten(&scene.items, &canvas);
            for (i, a) in placed.iter().enumerate() {
                for b in placed.iter().skip(i + 1) {
                    assert_ne!(
                        a.rect, b.rect,
                        "{name}: {} and {} are exactly on top of each other",
                        a.path, b.path
                    );
                    if a.rect.intersect(&b.rect).is_some() {
                        // The only overlap a layout is allowed is an item
                        // sitting inside the one below it: that is the pip.
                        assert!(
                            b.rect.inside(&a.rect),
                            "{name}: {} and {} overlap without one being inside the other",
                            a.path,
                            b.path
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn applying_a_layout_twice_lands_on_the_same_items() {
        let layout = builtin("pip-bottom-right").unwrap();
        let canvas = Canvas::default();
        let first = apply(&layout, &all_sources(&layout), canvas).unwrap();
        let mut bigger = all_sources(&layout);
        bigger.insert("inset".into(), Value::from(0.5));
        let second = apply_into(&layout, &bigger, canvas, first.id, None).unwrap();
        let ids = |s: &Scene| s.items.iter().map(|i| i.id).collect::<Vec<_>>();
        assert_eq!(
            ids(&first),
            ids(&second),
            "the inset must stay the same item"
        );
        let inset = |s: &Scene| s.items.last().unwrap().transform.frame.unwrap().w;
        assert!(inset(&second) > inset(&first), "the inset did not grow");
    }

    #[test]
    fn two_scenes_using_the_same_layout_do_not_share_item_ids() {
        let layout = builtin("full").unwrap();
        let a = apply(&layout, &all_sources(&layout), Canvas::default()).unwrap();
        let b = apply(&layout, &all_sources(&layout), Canvas::default()).unwrap();
        assert_ne!(a.items[0].id, b.items[0].id);
    }

    #[test]
    fn the_pip_lands_where_the_arithmetic_says_it_should() {
        let canvas = Canvas::default();
        let layout = builtin("pip-bottom-right").unwrap();
        let mut values = all_sources(&layout);
        values.insert("inset".into(), Value::from(0.25));
        values.insert("gap".into(), Value::from(0.05));
        let scene = apply(&layout, &values, canvas).unwrap();
        let placed = flatten(&scene.items, &canvas);
        // inset 0.25 of 1920x1080 is 480x270; a gap of 0.05 is 96 and 54 in.
        let want = Rect::new(1920.0 - 480.0 - 96.0, 1080.0 - 270.0 - 54.0, 480.0, 270.0);
        let got = placed.last().unwrap().rect;
        assert!(
            (got.x - want.x).abs() < EPSILON
                && (got.y - want.y).abs() < EPSILON
                && (got.w - want.w).abs() < EPSILON
                && (got.h - want.h).abs() < EPSILON,
            "the inset landed at {:?}, wanted {:?}",
            got.rounded(),
            want.rounded()
        );
    }

    #[test]
    fn a_slot_nobody_filled_in_is_left_out_rather_than_drawn_empty() {
        let layout = builtin("quad").unwrap();
        let mut values = Values::new();
        values.insert("a".into(), Value::from("cam1"));
        values.insert("b".into(), Value::from("cam2"));
        let scene = apply(&layout, &values, Canvas::default()).unwrap();
        assert_eq!(scene.items.len(), 2, "{:#?}", scene.items);
    }

    #[test]
    fn a_layout_that_needs_a_source_says_so_by_name() {
        let layout = builtin("full").unwrap();
        let err = apply(&layout, &Values::new(), Canvas::default())
            .unwrap_err()
            .to_string();
        assert!(err.contains("--values a="), "{err}");
    }

    #[test]
    fn a_value_the_layout_does_not_take_lists_the_ones_it_does() {
        let layout = builtin("full").unwrap();
        let mut values = all_sources(&layout);
        values.insert("wobble".into(), Value::from("x"));
        let err = apply(&layout, &values, Canvas::default())
            .unwrap_err()
            .to_string();
        assert!(err.contains("no parameter called \"wobble\""), "{err}");
        assert!(err.contains("It takes: a"), "{err}");
    }

    #[test]
    fn values_come_off_the_command_line_with_numbers_as_numbers() {
        let values = parse_values("a=cam1, b=cam2 ,inset=0.4").unwrap();
        assert_eq!(values["a"], Value::from("cam1"));
        assert_eq!(values["inset"], Value::from(0.4));
        assert!(parse_values("a")
            .unwrap_err()
            .to_string()
            .contains("name=value"));
    }

    #[test]
    fn an_applied_layout_carries_no_bindings_and_validates_clean() {
        for name in NAMES {
            let layout = builtin(name).unwrap();
            let canvas = Canvas::default();
            let scene = apply(&layout, &all_sources(&layout), canvas).unwrap();
            for item in scene.walk() {
                assert!(
                    item.bind.is_empty(),
                    "{name}: {:?} kept its bindings",
                    item.name
                );
            }
            let findings = validate::scene(&scene, &canvas);
            assert!(
                !validate::has_errors(&findings),
                "{name} does not validate: {findings:#?}"
            );
        }
    }

    #[test]
    fn a_graphic_slot_takes_its_params_from_the_values() {
        let layout = builtin("l-shape").unwrap();
        let mut values = all_sources(&layout);
        values.insert("headline".into(), Value::from("Evening service"));
        let scene = apply(&layout, &values, Canvas::default()).unwrap();
        let text = serde_json::to_string(&scene).unwrap();
        assert!(text.contains("Evening service"), "{text}");
        assert!(
            !text.contains("{{"),
            "a binding was left unresolved: {text}"
        );
    }
}
