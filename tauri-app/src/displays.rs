//! The displays this computer has, as the programme output menu lists them,
//! and the one the operator chose, kept across restarts.
//!
//! A display is told apart by its name and where it sits on the desktop, so
//! two projectors of the same model on one machine are still two choices, and
//! a projector plugged back in where it was is the same choice it was.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Monitor};

const FILE: &str = "programme-output.json";

/// One connected display, in physical pixels.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Display {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub primary: bool,
}

impl Display {
    fn of(m: &Monitor, index: usize, primary: Option<&Monitor>) -> Display {
        let name = m.name().filter(|n| !n.trim().is_empty()).cloned().unwrap_or_else(|| format!("Display {}", index + 1));
        let (pos, size) = (m.position(), m.size());
        let primary = primary.is_some_and(|p| p.position() == pos && p.size() == size);
        Display { name: friendly(&name), x: pos.x, y: pos.y, width: size.width, height: size.height, primary }
    }

    /// What the menu says: a name, the native resolution, and which one the
    /// operator is looking at.
    pub fn label(&self) -> String {
        let main = if self.primary { " (main display)" } else { "" };
        format!("{}, {} × {}{main}", self.name, self.width, self.height)
    }

    /// The same display: its name and its place. Its resolution may change
    /// (a projector renegotiating) and it is still the one chosen.
    pub fn same(&self, other: &Display) -> bool {
        self.name == other.name && self.x == other.x && self.y == other.y
    }
}

/// Windows names a display `\\.\DISPLAY2`; the number is what a person
/// recognises from the system's own display settings.
fn friendly(name: &str) -> String {
    match name.strip_prefix(r"\\.\DISPLAY") {
        Some(n) => format!("Display {n}"),
        None => name.to_string(),
    }
}

/// Every display connected now, the main one first.
pub fn connected(app: &AppHandle) -> Vec<Display> {
    let primary = app.primary_monitor().ok().flatten();
    let mut list: Vec<Display> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, m)| Display::of(m, i, primary.as_ref()))
        .collect();
    list.sort_by_key(|d| (!d.primary, d.x, d.y));
    list
}

/// The choice as it is kept: the display, and whether the output was open
/// when the app last stopped, so it comes back on that display by itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    #[serde(default)]
    pub display: Option<Display>,
    #[serde(default)]
    pub open: bool,
}

pub fn load(app: &AppHandle) -> Choice {
    let Ok(dir) = crate::settings::data_dir(app) else { return Choice::default() };
    std::fs::read_to_string(dir.join(FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(app: &AppHandle, choice: &Choice) {
    let Ok(dir) = crate::settings::data_dir(app) else { return };
    if let Ok(text) = serde_json::to_string_pretty(choice) {
        if let Err(e) = std::fs::write(dir.join(FILE), text) {
            eprintln!("[desktop] the programme output display was not remembered: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(name: &str, x: i32, w: u32) -> Display {
        Display { name: name.into(), x, y: 0, width: w, height: w * 9 / 16, primary: false }
    }

    #[test]
    fn a_display_is_its_name_and_place_whatever_its_resolution() {
        assert!(d("EPSON", 1920, 1920).same(&d("EPSON", 1920, 1280)));
        assert!(!d("EPSON", 1920, 1920).same(&d("EPSON", 3840, 1920)), "two of one model are two displays");
        assert_eq!(d("EPSON", 0, 1920).label(), "EPSON, 1920 × 1080");
        assert_eq!(friendly(r"\\.\DISPLAY2"), "Display 2");
    }

    #[test]
    fn an_old_or_empty_file_reads_as_no_choice() {
        assert_eq!(serde_json::from_str::<Choice>("{}").unwrap(), Choice::default());
        let kept = Choice { display: Some(d("LG", 1920, 3840)), open: true };
        let back: Choice = serde_json::from_str(&serde_json::to_string(&kept).unwrap()).unwrap();
        assert_eq!(back, kept);
    }
}
