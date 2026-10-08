//! The params schemas the kind table publishes, so the CLI, the MCP server, the
//! web UI and a third party client build the same form from the same source.

use serde_json::{json, Value};

/// The schema of `type_id`'s params, for the kinds that publish one.
pub fn params(type_id: &str) -> Option<Value> {
    match type_id {
        "text/source" => Some(super::text::schema()),
        "ticker/source" => Some(super::ticker::schema()),
        "template/source" => Some(super::template::schema()),
        "html/graphic" => Some(super::html::schema()),
        "image/source" => Some(image()),
        "file/source" => Some(file()),
        _ => None,
    }
}

fn alpha(what: &str) -> Value {
    json!({
        "description": format!(
            "Whether the {what}'s transparency is kept. `auto` (or left out) keeps it when the file has \
             it; `false` draws the {what} flat through the compositor like any other source."
        ),
        "anyOf": [{ "type": "boolean" }, { "const": "auto" }]
    })
}

fn image() -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "image/source params",
        "type": "object",
        "properties": {
            "fps": {
                "description": "Pictures a second for a numbered sequence such as frames/%04d.png. 1 to 120, 25 when left out.",
                "type": "integer", "minimum": 1, "maximum": 120
            },
            "alpha": alpha("picture")
        }
    })
}

fn file() -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "file/source params",
        "type": "object",
        "properties": {
            "alpha": alpha("clip"),
            "at_end": {
                "title": "At the end",
                "description": "What the clip does when its last frame has gone out. Repeat plays it again from \
                                the start with no gap and no reconnect. Hold the last frame, the default, keeps it \
                                up until the clip is scrubbed or restarted. Leave the scene holds it too, and if the \
                                clip is on air the programme moves to the scene armed in Preview, or else to what \
                                was on air before it. Changing it does not restart the clip.",
                "type": "string",
                "enum": ["repeat", "hold", "leave"],
                "x-gmx-labels": ["Repeat", "Hold the last frame", "Leave the scene"],
                "default": "hold"
            }
        }
    })
}
