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
}
