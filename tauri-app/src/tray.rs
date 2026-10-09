//! The tray while the app runs: a tooltip, an icon and the first lines of
//! its menu that say what the mixer is running. See `background`.

use tauri::image::Image;
use tauri::menu::{IsMenuItem, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::{AppHandle, Wry};

pub const STOP_ALL_ID: &str = "stop-all";

/// "GodwinMix is running in the background: streaming to YouTube, recording".
pub fn tooltip(lines: &[String], away: bool) -> String {
    let lead = if away { "GodwinMix is running in the background" } else { "GodwinMix" };
    if lines.is_empty() {
        return format!("{lead}: nothing is streaming or recording");
    }
    format!("{lead}: {}", lines.join(", "))
}

pub fn paint(app: &AppHandle, lines: &[String], away: bool) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let _ = tray.set_tooltip(Some(&tooltip(lines, away)));
    if let Some(icon) = app.default_window_icon() {
        let icon = if lines.is_empty() { icon.clone().to_owned() } else { live_icon(icon) };
        let _ = tray.set_icon(Some(icon));
    }
    if let Ok(menu) = menu(app, lines) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// The app's icon with a red dot in its corner, while anything is running.
pub fn live_icon(icon: &Image<'_>) -> Image<'static> {
    let (w, h) = (icon.width(), icon.height());
    let mut rgba = icon.rgba().to_vec();
    let r = (w.min(h) as f32) * 0.22;
    let (cx, cy) = (w as f32 - r - 1.0, h as f32 - r - 1.0);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            if dx * dx + dy * dy <= r * r {
                let at = ((y * w + x) * 4) as usize;
                rgba[at..at + 4].copy_from_slice(&[0xff, 0x2a, 0x36, 0xff]);
            }
        }
    }
    Image::new(&rgba, w, h).to_owned()
}

fn menu(app: &AppHandle, lines: &[String]) -> tauri::Result<tauri::menu::Menu<Wry>> {
    let mut said: Vec<tauri::menu::MenuItem<Wry>> = Vec::new();
    let shown = if lines.is_empty() { vec!["Nothing is streaming or recording".to_string()] } else { lines.to_vec() };
    for (n, line) in shown.iter().take(6).enumerate() {
        said.push(MenuItemBuilder::with_id(format!("running-{n}"), line).enabled(false).build(app)?);
    }
    let show = MenuItemBuilder::with_id("show", "Show GodwinMix").build(app)?;
    let stop = MenuItemBuilder::with_id(STOP_ALL_ID, "Stop all streaming").enabled(!lines.is_empty()).build(app)?;
    let connect = MenuItemBuilder::with_id("connect", "Connect to a mixer...").build(app)?;
    let browser = MenuItemBuilder::with_id(crate::browser::MENU_ID, crate::browser::MENU_TITLE).build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let (sep1, sep2) = (PredefinedMenuItem::separator(app)?, PredefinedMenuItem::separator(app)?);
    let mut items: Vec<&dyn IsMenuItem<Wry>> = said.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    items.extend([&sep1 as &dyn IsMenuItem<Wry>, &show, &stop, &sep2, &connect, &browser, &quit]);
    MenuBuilder::new(app).items(&items).build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tooltip_says_what_runs_and_whether_the_window_is_gone() {
        assert_eq!(tooltip(&[], false), "GodwinMix: nothing is streaming or recording");
        let lines = vec!["Streaming to YouTube, live for 1:56:23".to_string(), "Recording, for 1:05".to_string()];
        assert_eq!(
            tooltip(&lines, true),
            "GodwinMix is running in the background: Streaming to YouTube, live for 1:56:23, Recording, for 1:05"
        );
    }

    #[test]
    fn the_live_icon_has_a_red_corner_and_keeps_its_size() {
        let pixels = vec![0u8; 32 * 32 * 4];
        let plain = Image::new(&pixels, 32, 32);
        let live = live_icon(&plain);
        assert_eq!((live.width(), live.height()), (32, 32));
        let corner = ((28 * 32 + 28) * 4) as usize;
        assert_eq!(&live.rgba()[corner..corner + 4], &[0xff, 0x2a, 0x36, 0xff]);
        assert_eq!(&live.rgba()[0..4], &[0, 0, 0, 0], "the rest is left alone");
    }
}
