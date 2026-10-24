//! The main window coming out of full screen the size it went in.
//!
//! A page element asking for full screen (the flow map's, the headend
//! view's) makes the runtime put the whole window in full screen, and on
//! Windows a window that was maximized came back out at the size of the
//! screen without being maximized: it reached under the taskbar until it
//! was maximized by hand. So when the window leaves full screen covering
//! the whole work area or more and is not maximized, it is maximized, which
//! is what it was. The menu bar is hidden while it is full screen, as the
//! programme's own full screen does (`output::main_fullscreen`).
//!
//! It also owns the Window menu's Toggle Full Screen. The runtime's own item
//! for it is a macOS one: on Windows and Linux it is listed as unsupported
//! and does nothing, so the menu item and F11 did nothing there either. Off
//! macOS the item is the shell's own, and the shell puts the window in and
//! out of full screen itself.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{MenuItemBuilder, PredefinedMenuItem, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Manager, Wry};

/// The shell's own Toggle Full Screen, off macOS.
pub const MENU_ID: &str = "window-fullscreen";

/// The Window submenu, the same in the shell's menu and the page's.
pub fn window_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let sub = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?);
    if cfg!(target_os = "macos") {
        return sub.item(&PredefinedMenuItem::fullscreen(app, None)?).build();
    }
    let toggle = MenuItemBuilder::with_id(MENU_ID, "Toggle Full Screen").accelerator("F11").build(app)?;
    sub.item(&toggle).build()
}

/// Toggle Full Screen was picked. The menu bar is hidden and shown again by
/// `resized`, which sees the change whichever way it was made.
pub fn toggle(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    let on = !w.is_fullscreen().unwrap_or(false);
    if let Err(e) = w.set_fullscreen(on) {
        eprintln!("[desktop] the window could not go {} full screen: {e}", if on { "into" } else { "out of" });
    }
}

static FULL: AtomicBool = AtomicBool::new(false);

/// A rectangle: x, y, width, height, in physical pixels.
type Rect = (i32, i32, u32, u32);

/// Whether a window that is not maximized, at `window`, covers the whole of
/// `work`, the monitor less its taskbar. A maximized window does too, and
/// a little more for its borders; that is the state it should be in.
pub fn spills(window: Rect, work: Rect) -> bool {
    window.2 >= work.2 && window.3 >= work.3
}

/// The main window was resized: see whether it just left full screen.
pub fn resized(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    let full = w.is_fullscreen().unwrap_or(false);
    if FULL.swap(full, Ordering::SeqCst) == full {
        return;
    }
    if full {
        let _ = w.hide_menu();
        return;
    }
    let _ = w.show_menu();
    let (Ok(at), Ok(size), Ok(Some(monitor))) = (w.outer_position(), w.outer_size(), w.current_monitor()) else { return };
    let area = monitor.work_area();
    let work = (area.position.x, area.position.y, area.size.width, area.size.height);
    // Windows only: that is where it was seen, and elsewhere a window the
    // size of the screen may be what the person made it.
    if cfg!(windows) && !w.is_maximized().unwrap_or(false) && spills((at.x, at.y, size.width, size.height), work) {
        let _ = w.maximize();
    }
}

#[cfg(test)]
mod tests {
    use super::spills;

    #[test]
    fn a_window_the_size_of_the_screen_spills_and_a_smaller_one_does_not() {
        let work = (0, 0, 1920, 1032);
        assert!(spills((0, 0, 1920, 1080), work), "the screen's size, under the taskbar");
        assert!(spills((-8, -8, 1936, 1048), work), "a maximized window's rect, borders and all");
        assert!(!spills((200, 100, 1280, 800), work), "a window of its own size stays as it is");
        assert!(!spills((0, 0, 1920, 900), work), "full width alone is a choice");
    }
}
