//! A page driven by hand: the control lines `--control` reads, and where on
//! the page they land.
//!
//! The mixer writes one JSON object a line on stdin, exactly as
//! `godwinmix_protocol::page::PageEvent` serialises:
//!
//! ```text
//!   {"type":"down","x":0.5,"y":0.25,"button":"left","count":1}
//!   {"type":"key","code":13}
//!   {"type":"text","text":"hello"}
//!   {"type":"go","url":"https://example.com"}
//! ```
//!
//! A point is a share of the page, 0.0 to 1.0 across and down, and becomes a
//! pixel of the page's own layout here, the one place that knows how big the
//! page is drawn. Nothing in this file touches CEF, so all of it is tested
//! without a browser; `page_cef.rs` hands the commands to Chromium.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    #[default]
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct Mods {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub meta: bool,
}

/// One control line.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Cmd {
    Move { x: f64, y: f64, #[serde(flatten)] mods: Mods },
    Down { x: f64, y: f64, #[serde(default)] button: Button, #[serde(default = "one")] count: u8, #[serde(flatten)] mods: Mods },
    Up { x: f64, y: f64, #[serde(default)] button: Button, #[serde(default = "one")] count: u8, #[serde(flatten)] mods: Mods },
    Wheel { x: f64, y: f64, #[serde(default)] dx: f64, #[serde(default)] dy: f64 },
    Leave,
    Key { code: u16, #[serde(default)] key: Option<String>, #[serde(flatten)] mods: Mods },
    Text { text: String },
    Go { url: String },
    Back,
    Forward,
    Reload,
}

fn one() -> u8 {
    1
}

/// The command on a line, or why there is none.
pub fn parse(line: &str) -> Result<Cmd, String> {
    serde_json::from_str(line.trim()).map_err(|e| format!("not a control line ({e})"))
}

/// A share of a side, as a pixel on it: 0.0 is the first pixel and 1.0 the
/// last, and anything outside is pinned to the edge.
pub fn to_pixel(share: f64, side: i32) -> i32 {
    if side <= 1 || !share.is_finite() {
        return 0;
    }
    let px = (share.clamp(0.0, 1.0) * side as f64).floor() as i32;
    px.min(side - 1)
}

/// A point on the page, in its layout pixels.
pub fn to_view(x: f64, y: f64, view: (i32, i32)) -> (i32, i32) {
    (to_pixel(x, view.0), to_pixel(y, view.1))
}

/// A browser's `wheel` delta as Chromium's: a positive `dy` scrolls down the
/// page and Chromium's positive delta scrolls up, so the sign turns over.
/// Rounded, and kept to what one event should move.
pub fn wheel_delta(d: f64) -> i32 {
    if !d.is_finite() {
        return 0;
    }
    (-d).round().clamp(-10_000.0, 10_000.0) as i32
}

/// Whether a key press also types a character, and which. Enter, Tab and
/// Backspace type control characters Chromium needs for a form to submit or
/// focus to move; a printable key types itself unless Ctrl, Alt or Meta make
/// it a shortcut.
pub fn typed_char(code: u16, key: Option<&str>, mods: Mods) -> Option<u16> {
    match code {
        13 => return Some(13),
        9 => return Some(9),
        8 => return Some(8),
        _ => {}
    }
    if mods.ctrl || mods.alt || mods.meta {
        return None;
    }
    let key = key?;
    let mut chars = key.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None; // "ArrowLeft", "Shift": a key name, not a character.
    }
    let mut buf = [0u16; 2];
    let units = c.encode_utf16(&mut buf);
    (units.len() == 1).then_some(units[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_land_on_pixels_of_the_layout() {
        assert_eq!(to_view(0.0, 0.0, (1280, 720)), (0, 0));
        assert_eq!(to_view(0.5, 0.5, (1280, 720)), (640, 360));
        // The far edge is the last pixel, never one past it.
        assert_eq!(to_view(1.0, 1.0, (1280, 720)), (1279, 719));
        // A page drawn at 0.6667 scale lays out on 1920 by 1080.
        assert_eq!(to_view(0.25, 0.75, (1920, 1080)), (480, 810));
        assert_eq!(to_view(-3.0, f64::NAN, (1280, 720)), (0, 0));
        assert_eq!(to_view(7.0, 0.5, (1280, 720)), (1279, 360));
    }

    #[test]
    fn lines_parse_as_the_mixer_writes_them() {
        assert_eq!(
            parse(r#"{"type":"down","x":0.5,"y":0.25,"button":"right","count":2}"#).unwrap(),
            Cmd::Down { x: 0.5, y: 0.25, button: Button::Right, count: 2, mods: Mods::default() }
        );
        assert_eq!(parse(r#"{"type":"up","x":0,"y":1}"#).unwrap(), Cmd::Up { x: 0.0, y: 1.0, button: Button::Left, count: 1, mods: Mods::default() });
        assert_eq!(
            parse(r#"{"type":"key","code":65,"key":"a","ctrl":true}"#).unwrap(),
            Cmd::Key { code: 65, key: Some("a".into()), mods: Mods { ctrl: true, ..Mods::default() } }
        );
        assert_eq!(parse(r#"{"type":"text","text":"日本"}"#).unwrap(), Cmd::Text { text: "日本".into() });
        assert_eq!(parse(r#"{"type":"go","url":"https://a.b"}"#).unwrap(), Cmd::Go { url: "https://a.b".into() });
        assert_eq!(parse(r#" {"type":"back"} "#).unwrap(), Cmd::Back);
        assert!(parse(r#"{"fields":{}}"#).is_err());
        assert!(parse("click 1 2").is_err());
    }

    #[test]
    fn a_scroll_down_is_a_negative_delta_for_chromium() {
        assert_eq!(wheel_delta(120.0), -120);
        assert_eq!(wheel_delta(-53.4), 53);
        assert_eq!(wheel_delta(f64::NAN), 0);
        assert_eq!(wheel_delta(1e9), -10_000);
    }

    #[test]
    fn which_presses_type_a_character() {
        let none = Mods::default();
        assert_eq!(typed_char(65, Some("a"), none), Some('a' as u16));
        assert_eq!(typed_char(65, Some("A"), Mods { shift: true, ..none }), Some('A' as u16));
        assert_eq!(typed_char(65, Some("a"), Mods { ctrl: true, ..none }), None);
        assert_eq!(typed_char(13, Some("Enter"), none), Some(13));
        assert_eq!(typed_char(37, Some("ArrowLeft"), none), None);
        assert_eq!(typed_char(229, None, none), None);
        // Outside the basic plane: sent as text instead.
        assert_eq!(typed_char(0, Some("😀"), none), None);
    }
}
