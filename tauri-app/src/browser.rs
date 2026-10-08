//! Open in browser: the mixer this window shows, in the system's own browser,
//! already signed in.
//!
//! The app starts its mixer on a port it takes fresh at every launch, with a
//! token it generated itself, so typing the address into a browser met a
//! token prompt for a token nobody had seen. This hands the browser the same
//! page with the token in the fragment, which the page stores and takes out
//! of the address bar before it draws.

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::MessageDialogKind;
use tauri_plugin_opener::OpenerExt;

use crate::Shell;

pub const MENU_ID: &str = "open-in-browser";
pub const MENU_TITLE: &str = "Open in browser";

pub fn open(app: &AppHandle) {
    let target = app.state::<Shell>().target.lock().unwrap().clone();
    let Some(target) = target else {
        crate::ui::tell(
            app,
            "No mixer yet",
            "The window is not showing a mixer yet. Connect to one, or wait for the mixer on this computer to start, then try again.",
            MessageDialogKind::Info,
        );
        return;
    };
    if let Err(e) = app.opener().open_url(target.browser_url(), None::<&str>) {
        let body = format!("{e}\n\nThe mixer is at {}.", target.base);
        crate::ui::tell(app, "Could not open the browser", &body, MessageDialogKind::Error);
    }
}
