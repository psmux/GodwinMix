//! The words a transition is spelled in, for every client at once.
//!
//! The core builds the curves; this module only knows the names, the params
//! each one reads and what a wrong value should say. It lives here so the
//! control server, the CLI, the MCP server and a third party client refuse
//! the same `{"type": "wipe", "params": {"direction": "sideways"}}` with the
//! same message and the same `data`.

use serde_json::{json, Map, Value};

/// Which way a `wipe`, `slide` or `push` travels.
pub const DIRECTIONS: &[&str] = &["left", "right", "up", "down"];

/// How progress is shaped over a transition. `ease-in-out` is the default and
/// is the curve `fade` has always had.
pub const EASINGS: &[&str] = &["linear", "ease-in", "ease-out", "ease-in-out"];

/// The colours `dip` takes by name. Anything else is `#rrggbb`.
pub const COLOURS: &[&str] = &["black", "white"];

/// What an item's `enter` and `exit` may be.
/// `hold` keeps the item drawn as it is for its duration and then takes it
/// away: the way out for a graphic that animates itself (an HTML template),
/// which needs to stay on the canvas while its own animation plays.
pub const ITEM_TRANSITIONS: &[&str] = &["cut", "fade", "slide", "zoom", "wipe", "hold"];

/// The edge an item slides or wipes in from, and out to.
pub const EDGES: &[&str] = &["left", "right", "top", "bottom"];

/// The params a built in transition reads, by name, for a picker and for the
/// reference page. A transition missing from here reads none.
pub fn params_of(type_id: &str) -> &'static [&'static str] {
    match type_id {
        "wipe" | "slide" | "push" => &["direction", "easing"],
        "zoom" | "zoom-out" | "box" => &["x", "y", "easing"],
        "dip" => &["colour", "easing"],
        "stinger" => &["clip", "cut_at_ms", "luma"],
        "fade" | "move" => &["easing"],
        _ => &[],
    }
}

/// A refusal a caller can act on: the sentence, and what to pick from.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub message: String,
    pub data: Map<String, Value>,
}

impl Refusal {
    fn new(message: String, key: &str, choices: &[&str]) -> Refusal {
        let mut data = Map::new();
        data.insert(key.to_string(), json!(choices));
        Refusal { message, data }
    }
}

/// Check the params of a built in transition. A plugin's are its own business.
pub fn check_params(type_id: &str, params: &Map<String, Value>) -> Result<(), Refusal> {
    if let Some(easing) = text(params, "easing") {
        one_of("easing", &easing, EASINGS, type_id)?;
    }
    match type_id {
        "wipe" | "slide" | "push" => match text(params, "direction") {
            Some(d) => one_of("direction", &d, DIRECTIONS, type_id),
            None => Ok(()),
        },
        "zoom" | "zoom-out" | "box" => check_point(params, type_id),
        "dip" => match text(params, "colour").or_else(|| text(params, "color")) {
            Some(c) if parse_colour(&c).is_none() => Err(Refusal::new(
                format!(
                    "a dip has no colour called {c:?}. Name one of {} or write it as \
                     #rrggbb, for example #1f6f4f.",
                    COLOURS.join(", ")
                ),
                "colours",
                COLOURS,
            )),
            _ => Ok(()),
        },
        _ => Ok(()),
    }
}

/// Check an item's `enter` or `exit` as a client sent it.
pub fn check_item(which: &str, value: &Value) -> Result<(), Refusal> {
    if value.is_null() {
        return Ok(());
    }
    let Some(object) = value.as_object() else {
        return Err(Refusal::new(
            format!("`{which}` is an object such as {{\"type\": \"slide\", \"edge\": \"left\"}}, or null to clear it."),
            "transitions",
            ITEM_TRANSITIONS,
        ));
    };
    let kind = text(object, "type").unwrap_or_default();
    if !ITEM_TRANSITIONS.contains(&kind.as_str()) {
        return Err(Refusal::new(
            format!(
                "an item cannot {which} with {kind:?}. It can use: {}.",
                ITEM_TRANSITIONS.join(", ")
            ),
            "transitions",
            ITEM_TRANSITIONS,
        ));
    }
    if let Some(edge) = text(object, "edge") {
        one_of("edge", &edge, EDGES, &kind)?;
    }
    if let Some(easing) = text(object, "easing") {
        one_of("easing", &easing, EASINGS, &kind)?;
    }
    Ok(())
}

/// `#rrggbb` or a name, as `0xAARRGGBB` with the alpha opaque.
pub fn parse_colour(text: &str) -> Option<u32> {
    let t = text.trim().to_lowercase();
    match t.as_str() {
        "black" => return Some(0xff00_0000),
        "white" => return Some(0xffff_ffff),
        _ => {}
    }
    let hex = t.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok().map(|rgb| 0xff00_0000 | rgb)
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params.get(key)?.as_str().map(|s| s.trim().to_lowercase())
}

fn one_of(key: &str, value: &str, choices: &[&str], kind: &str) -> Result<(), Refusal> {
    if choices.contains(&value) {
        return Ok(());
    }
    Err(Refusal::new(
        format!("{kind} has no {key} called {value:?}. Use one of: {}.", choices.join(", ")),
        &format!("{key}s"),
        choices,
    ))
}

fn check_point(params: &Map<String, Value>, kind: &str) -> Result<(), Refusal> {
    for key in ["x", "y"] {
        if let Some(v) = params.get(key) {
            if !v.as_f64().is_some_and(|f| (0.0..=1.0).contains(&f)) {
                let mut data = Map::new();
                data.insert("range".into(), json!([0.0, 1.0]));
                return Err(Refusal {
                    message: format!(
                        "{kind} takes `{key}` as a fraction of the canvas from 0 to 1 (0.5 is \
                         the centre); it was {v}."
                    ),
                    data,
                });
            }
        }
    }
    Ok(())
}

pub use catalogue::{TransitionCatalogue, TransitionEntry};

#[path = "transitions_catalogue.rs"]
mod catalogue;

#[cfg(test)]
#[path = "transitions_tests.rs"]
mod tests;
