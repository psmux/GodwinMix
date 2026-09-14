//! What `schemas/source.json` lets an operator change.

use serde_json::Value;

/// A rectangle in screen pixels, from the top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub monitor: u32,
    pub region: Option<Region>,
    pub show_cursor: bool,
    /// X11 only.
    pub display: String,
    /// Wayland only: a node id from a portal session the desktop already
    /// granted.
    pub node_id: String,
    pub label: String,
    pub element: String,
}

impl Settings {
    pub fn from(params: &Value) -> Settings {
        Settings {
            monitor: params
                .get("monitor")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(15) as u32,
            region: parse_region(&text(params, "region")),
            // The schema's default is true, so a missing key means true here.
            show_cursor: params.get("show_cursor").and_then(Value::as_bool).unwrap_or(true),
            display: text(params, "display"),
            node_id: text(params, "node_id"),
            label: text(params, "label"),
            element: text(params, "element"),
        }
    }

    /// Everything about a screen capture is a property of the capture that was
    /// opened, so any change reopens it. The pointer could be toggled live on
    /// some elements and not on others, and one behaviour on every platform is
    /// worth more than one saved freeze frame.
    pub fn needs_restart(&self, next: &Settings) -> bool {
        self.monitor != next.monitor
            || self.region != next.region
            || self.show_cursor != next.show_cursor
            || self.display != next.display
            || self.node_id != next.node_id
            || self.element != next.element
    }
}

fn text(params: &Value, key: &str) -> String {
    params.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string()
}

fn parse_region(text: &str) -> Option<Region> {
    let numbers: Vec<u32> = text.split(',').filter_map(|n| n.trim().parse().ok()).collect();
    match numbers[..] {
        [x, y, width, height] if width > 0 && height > 0 => {
            Some(Region { x, y, width, height })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_object_is_the_whole_first_monitor_with_a_pointer() {
        let s = Settings::from(&json!({}));
        assert_eq!(s.monitor, 0);
        assert!(s.region.is_none());
        assert!(s.show_cursor, "the schema's default is true and so is this");
    }

    #[test]
    fn a_region_is_read_and_a_broken_one_is_ignored() {
        assert_eq!(
            Settings::from(&json!({"region": "10,20,640,480"})).region,
            Some(Region { x: 10, y: 20, width: 640, height: 480 })
        );
        assert_eq!(Settings::from(&json!({"region": ""})).region, None);
        assert_eq!(Settings::from(&json!({"region": "10,20"})).region, None);
        assert_eq!(
            Settings::from(&json!({"region": "0,0,0,480"})).region,
            None,
            "a zero wide region is not a region"
        );
    }

    #[test]
    fn the_pointer_can_be_turned_off_which_is_what_lyrics_want() {
        assert!(!Settings::from(&json!({"show_cursor": false})).show_cursor);
    }

    #[test]
    fn every_setting_reopens_the_capture_except_the_label() {
        let a = Settings::from(&json!({}));
        assert!(!a.needs_restart(&Settings::from(&json!({"label": "Lyrics"}))));
        assert!(a.needs_restart(&Settings::from(&json!({"monitor": 1}))));
        assert!(a.needs_restart(&Settings::from(&json!({"show_cursor": false}))));
        assert!(a.needs_restart(&Settings::from(&json!({"region": "0,0,640,480"}))));
    }
}
