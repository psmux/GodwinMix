//! The params the built in transitions read, as the mixer holds them.
//!
//! Parsed once, when a take arrives, so nothing on the bind path reads JSON.
//! A value this build does not understand is the default rather than an
//! error: the control layer refuses a wrong one with the choices listed
//! (`godwinmix_protocol::transitions`), and what reaches here unchecked is a
//! collection written by an older build.

use super::Kind;
use godwinmix_protocol::requests::Transition as Request;

/// Which way a wipe, slide or push travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// In from the right edge, travelling left. What `plugins/wipe` means by
    /// left too, so the plugin and the built in agree.
    #[default]
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub fn parse(name: Option<&str>) -> Direction {
        match name.map(|s| s.trim().to_lowercase()).as_deref() {
            Some("right") => Direction::Right,
            Some("up") => Direction::Up,
            Some("down") => Direction::Down,
            _ => Direction::Left,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Direction::Left => "left",
            Direction::Right => "right",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }

    /// The way it travels, one unit long.
    pub fn vector(self) -> (f64, f64) {
        match self {
            Direction::Left => (-1.0, 0.0),
            Direction::Right => (1.0, 0.0),
            Direction::Up => (0.0, -1.0),
            Direction::Down => (0.0, 1.0),
        }
    }
}

/// A point on the canvas, in thousandths of its width and height. Whole
/// numbers so a `Kind` stays comparable; a thousandth of 1920 is two pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

impl Default for Point {
    fn default() -> Self {
        Point { x: 500, y: 500 }
    }
}

impl Point {
    fn from_fractions(x: Option<f64>, y: Option<f64>) -> Point {
        let part = |v: Option<f64>| (v.unwrap_or(0.5).clamp(0.0, 1.0) * 1000.0).round() as u16;
        Point { x: part(x), y: part(y) }
    }

    /// Where it is on a canvas of this size, in pixels.
    pub fn on(self, canvas: (i32, i32)) -> (f64, f64) {
        (self.x as f64 / 1000.0 * canvas.0 as f64, self.y as f64 / 1000.0 * canvas.1 as f64)
    }
}

/// The colour of the slate during a dip, as `0xAARRGGBB`.
pub const BLACK: u32 = 0xff00_0000;

/// The built in kind a request names, or `None` for a name that is not one of
/// the new built in transitions.
pub(super) fn kind_of(type_id: &str, request: &Request) -> Option<Kind> {
    let direction = || Direction::parse(request.param_str("direction").as_deref());
    let point = || Point::from_fractions(request.param_f64("x"), request.param_f64("y"));
    Some(match type_id {
        "wipe" => Kind::Wipe { direction: direction() },
        "slide" => Kind::Slide { direction: direction() },
        "push" => Kind::Push { direction: direction() },
        "zoom" => Kind::Zoom { point: point() },
        "zoom-out" => Kind::ZoomOut { point: point() },
        "box" => Kind::Box { point: point() },
        "dip" => Kind::Dip {
            colour: request
                .param_str("colour")
                .or_else(|| request.param_str("color"))
                .and_then(|c| godwinmix_protocol::transitions::parse_colour(&c))
                .unwrap_or(BLACK),
        },
        _ => return None,
    })
}
