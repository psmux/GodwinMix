//! Where an item lands when nobody said: over the whole canvas, for the kinds
//! that are designed on the whole canvas.
//!
//! An SVG template is laid out on 1920 by 1080 with its lower third already
//! at the bottom left and its bug already in the corner. Dropped into the next
//! free cell of a grid it is a lower third squeezed into half the picture,
//! which is what a small model did every time it left `transform` out. So a
//! template placed with no transform covers the canvas, the place it was
//! drawn for.

use godwinmix_core::scene::document::{Frame, Transform, Vec2};
use godwinmix_core::scene::Collection;
use godwinmix_protocol::types::SourceStatus;

/// The source kinds whose picture is a whole canvas with its parts in place.
const WHOLE_CANVAS_KINDS: [&str; 1] = ["template/source"];

/// True when a source of this status is drawn on the whole canvas.
pub fn covers_canvas(status: &SourceStatus) -> bool {
    status
        .extra("type")
        .and_then(|t| t.as_str())
        .is_some_and(|kind| WHOLE_CANVAS_KINDS.contains(&kind))
}

/// A transform that fills the collection's canvas exactly.
pub fn transform(doc: &Collection) -> Transform {
    Transform {
        position: Vec2::new(0.0, 0.0),
        frame: Some(Frame::new(doc.canvas.width as f64, doc.canvas.height as f64)),
        ..Transform::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status(kind: &str) -> SourceStatus {
        serde_json::from_value(json!({
            "id": "lower", "name": "lower", "uri": "template:news-lower-third", "state": "live",
            "has_video": true, "has_audio": false, "type": kind
        }))
        .expect("a status")
    }

    #[test]
    fn a_template_covers_the_canvas_and_a_camera_does_not() {
        assert!(covers_canvas(&status("template/source")));
        assert!(!covers_canvas(&status("test/source")));
    }

    #[test]
    fn the_transform_is_the_canvas() {
        let canvas = godwinmix_core::scene::document::Canvas { width: 1280, height: 720, fps: 25 };
        let doc = Collection::new("test", canvas);
        let t = transform(&doc);
        let frame = t.frame.expect("a frame");
        assert_eq!((frame.w, frame.h), (doc.canvas.width as f64, doc.canvas.height as f64));
        assert_eq!((t.position.x, t.position.y), (0.0, 0.0));
    }
}
