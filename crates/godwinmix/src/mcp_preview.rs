//! `preview_frame` answers as a picture the model can look at.
//!
//! `scene.preview.frame` returns its JPEG as base64 inside a JSON record,
//! which suits a browser. Handed to a model as text it is thousands of
//! tokens of noise. This turns it into MCP image content with a one line
//! caption, the same as `snapshot`, so an agent that just placed a graphic on
//! the armed scene can see it and fix it.

use serde_json::{json, Value};

/// The MCP content for a `scene.preview.frame` body, or `None` when the body
/// carries no picture (and is passed on as text).
pub fn content(text: &str) -> Option<Value> {
    let body: Value = serde_json::from_str(text.trim()).ok()?;
    let data = body.get("image")?.as_str()?;
    // A gallery preview says what it is in its own caption.
    let caption = body.get("caption").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| {
        format!(
            "the armed scene {} at {}x{}",
            body.get("scene").and_then(Value::as_str).unwrap_or("?"),
            body.get("width").and_then(Value::as_u64).unwrap_or(0),
            body.get("height").and_then(Value::as_u64).unwrap_or(0)
        )
    });
    Some(json!({ "content": [
        { "type": "image", "data": data, "mimeType": "image/jpeg" },
        { "type": "text", "text": caption }
    ] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_becomes_an_image_and_anything_else_stays_text() {
        let body = r#"{"scene":"news","width":640,"height":360,"image":"AAAA","encoding":"base64","format":"jpeg"}"#;
        let c = content(body).expect("a picture");
        assert_eq!(c["content"][0]["type"], "image");
        assert_eq!(c["content"][0]["data"], "AAAA");
        assert_eq!(c["content"][1]["text"], "the armed scene news at 640x360");
        assert!(content(r#"{"ok":true}"#).is_none());
    }
}
