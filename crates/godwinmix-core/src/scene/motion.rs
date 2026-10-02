//! An item's `enter` and `exit`, as the scene document stores them.
//!
//! ```json
//! "enter": {"type": "slide", "edge": "left", "duration_ms": 400, "easing": "ease-out"},
//! "exit":  {"type": "slide", "edge": "left", "duration_ms": 300}
//! ```
//!
//! Played when the item is shown or hidden while its scene is on air, and,
//! with `on_take`, when a scene holding it is taken. The words are the ones
//! `godwinmix_protocol::transitions` lists, so a client that checks against
//! the protocol and the document that stores the answer agree.

use crate::mixer::transition::easing::Easing;
use crate::mixer::transition::item::{Edge, ItemKind, ItemMotion};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The default length of an item's entrance or exit.
pub const ITEM_DEFAULT_MS: u32 = 300;

/// One way on or off the canvas for one item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ItemTransition {
    /// cut, fade, slide, zoom or wipe.
    #[serde(rename = "type")]
    pub kind: ItemTransitionKind,
    /// How long it takes, in milliseconds. 300 by default, ten seconds at
    /// most.
    #[serde(default = "default_ms")]
    pub duration_ms: u32,
    /// linear, ease-in, ease-out or ease-in-out (the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    /// For slide and wipe, the canvas edge it comes in from or goes out to:
    /// left (the default), right, top or bottom.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<ItemEdge>,
    /// Also play it when a scene holding this item is taken, in place of the
    /// scene's own transition for this item.
    #[serde(default, skip_serializing_if = "is_false")]
    pub on_take: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ItemTransitionKind {
    Cut,
    Fade,
    Slide,
    Zoom,
    Wipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ItemEdge {
    Left,
    Right,
    Top,
    Bottom,
}

fn default_ms() -> u32 {
    ITEM_DEFAULT_MS
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl ItemTransition {
    /// What the mixer plays. The duration is held at the ceiling every
    /// transition has, and the validator says so.
    pub fn motion(&self) -> ItemMotion {
        ItemMotion {
            kind: match self.kind {
                ItemTransitionKind::Cut => ItemKind::Cut,
                ItemTransitionKind::Fade => ItemKind::Fade,
                ItemTransitionKind::Slide => ItemKind::Slide,
                ItemTransitionKind::Zoom => ItemKind::Zoom,
                ItemTransitionKind::Wipe => ItemKind::Wipe,
            },
            duration_ms: (self.duration_ms as u64).min(crate::mixer::transition::MAX_DURATION_MS),
            easing: Easing::parse(self.easing.as_deref()),
            edge: match self.edge {
                Some(ItemEdge::Right) => Edge::Right,
                Some(ItemEdge::Top) => Edge::Top,
                Some(ItemEdge::Bottom) => Edge::Bottom,
                _ => Edge::Left,
            },
            on_take: self.on_take,
        }
    }
}

/// What `scene.validate` says about the items' enters and exits. None of it
/// stops a take: a motion that is too long is held at the ceiling, an easing
/// nobody knows is the default, and a group's own motion is not played.
pub fn findings(scene: &super::document::Scene) -> Vec<super::validate::Finding> {
    use super::validate::{Finding, Severity};
    let mut out = Vec::new();
    let max = crate::mixer::transition::MAX_DURATION_MS;
    for item in scene.walk() {
        let label = item.name.clone().unwrap_or_else(|| item.id.to_string());
        let group = matches!(item.content, super::document::Content::Children { .. });
        for (which, t) in [("enter", &item.enter), ("exit", &item.exit)] {
            let Some(t) = t else { continue };
            let mut found = |severity, code: &str, message: String, detail| {
                out.push(Finding {
                    severity,
                    code: code.into(),
                    scene: Some(scene.id),
                    items: vec![item.id],
                    message,
                    detail,
                })
            };
            if t.duration_ms as u64 > max {
                found(
                    Severity::Warning,
                    "scene.motion_long",
                    format!("{label}'s {which} runs {} ms; it is held at {max} ms. Shorten it.", t.duration_ms),
                    serde_json::json!({ "duration_ms": t.duration_ms, "max_ms": max }),
                );
            }
            let easings = godwinmix_protocol::transitions::EASINGS;
            if let Some(e) = t.easing.as_deref().filter(|e| !easings.contains(e)) {
                found(
                    Severity::Warning,
                    "scene.motion_easing",
                    format!("{label}'s {which} has no easing called {e:?}; it plays as ease-in-out. Use one of: {}.", easings.join(", ")),
                    serde_json::json!({ "easings": easings }),
                );
            }
            if group {
                found(
                    Severity::Info,
                    "scene.motion_group",
                    format!("{label} is a group, and a group's own {which} is not played. Give it to the items inside instead."),
                    serde_json::Value::Null,
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lower_third_reads_and_writes_in_the_words_the_protocol_lists() {
        let t: ItemTransition =
            serde_json::from_str(r#"{"type": "slide", "edge": "left", "easing": "ease-out"}"#).expect("a slide");
        assert_eq!(t.duration_ms, ITEM_DEFAULT_MS);
        let m = t.motion();
        assert_eq!((m.kind, m.edge, m.easing), (ItemKind::Slide, Edge::Left, Easing::Out));
        let back = serde_json::to_value(&t).expect("serialises");
        assert_eq!(back["type"], "slide");
        assert!(back.get("on_take").is_none(), "false is left out of the file");
        for name in godwinmix_protocol::transitions::ITEM_TRANSITIONS {
            let v = serde_json::json!({ "type": name });
            assert!(serde_json::from_value::<ItemTransition>(v).is_ok(), "{name}");
        }
        let wrong = serde_json::from_str::<ItemTransition>(r#"{"type": "spin"}"#).unwrap_err();
        assert!(wrong.to_string().contains("slide"), "{wrong}");
    }

    #[test]
    fn the_validator_names_a_motion_too_long_and_an_easing_nobody_has() {
        use crate::scene::document::{Content, Item, Scene};
        let mut item = Item::new(Content::Source { source: "cam".into() });
        item.enter = Some(ItemTransition {
            kind: ItemTransitionKind::Fade,
            duration_ms: 20_000,
            easing: Some("bounce".into()),
            edge: None,
            on_take: false,
        });
        let mut scene = Scene::new("show");
        scene.items.push(item);
        let codes: Vec<String> = findings(&scene).into_iter().map(|f| f.code).collect();
        assert_eq!(codes, vec!["scene.motion_long", "scene.motion_easing"]);
    }
}
