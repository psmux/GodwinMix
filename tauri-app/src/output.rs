//! The programme on a display: a borderless full screen window on the
//! projector or monitor the operator chose, showing the core's /programme/
//! page and nothing else.
//!
//! The page asks for this with `godwinmix://programme-output`, from View >
//! Programme on a display or the Display button on the Programme monitor, and
//! the shell answers with a native menu of the connected displays. The
//! window is the shell's, so moving, reloading or covering the main window
//! does not touch it. Closing it (Esc on it, Stop in the menu, or the display
//! going away) closes a window and nothing else: the core is asked for no
//! change, so streams, recordings and the programme carry on.
//!
//! The choice is kept in `programme-output.json`. While the output is wanted a
//! watch looks at the displays every two seconds: a display unplugged takes
//! its window with it rather than letting the window land on the operator's
//! screen, and the same display plugged back in, or found at the next start,
//! gets the output back.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

use crate::displays::{self, Choice, Display};
use crate::Shell;

pub const LABEL: &str = "programme-output";
const PREFIX: &str = "output|";
const STOP: &str = "output|stop";

static WATCHING: AtomicBool = AtomicBool::new(false);

/// The native menu of displays, at the pointer over the main window.
pub fn menu(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let list = displays::connected(&handle);
        let showing = handle.get_webview_window(LABEL).is_some();
        let chosen = displays::load(&handle).display;
        let build = || -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
            let mut m = MenuBuilder::new(&handle);
            for (i, d) in list.iter().enumerate() {
                let on = showing && chosen.as_ref().is_some_and(|c| c.same(d));
                m = m.item(&CheckMenuItemBuilder::with_id(format!("{PREFIX}{i}"), d.label()).checked(on).build(&handle)?);
            }
            m = m.item(&PredefinedMenuItem::separator(&handle)?);
            m.item(&MenuItemBuilder::with_id(STOP, "Stop the programme output").enabled(showing).build(&handle)?).build()
        };
        let Some(window) = handle.get_webview_window("main") else { return };
        match build() {
            Ok(m) => {
                let _ = window.popup_menu(&m);
            }
            Err(e) => eprintln!("[desktop] the display menu could not be built: {e}"),
        }
    });
}

/// A display menu item was clicked. False for an id that is not one.
pub fn on_menu(app: &AppHandle, id: &str) -> bool {
    if id == STOP {
        stop(app);
        return true;
    }
    let Some(index) = id.strip_prefix(PREFIX).and_then(|i| i.parse::<usize>().ok()) else { return false };
    let Some(display) = displays::connected(app).into_iter().nth(index) else { return true };
    displays::save(app, &Choice { display: Some(display), open: true });
    close_window(app);
    // Off the menu's own callback: a window made inside it can deadlock
    // WebView2 on Windows.
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        reconcile(&handle);
        watch(&handle);
    });
    true
}

/// Stop the output and forget that it was open; the display stays chosen.
pub fn stop(app: &AppHandle) {
    let mut choice = displays::load(app);
    choice.open = false;
    displays::save(app, &choice);
    close_window(app);
}

/// Once connected: bring the output back if it was open when the app stopped.
pub fn restore(app: &AppHandle, page: &Url) {
    let target = app.state::<Shell>().target.lock().unwrap().clone();
    let on_core = target.is_some_and(|t| page.as_str().starts_with(&t.base));
    if on_core && displays::load(app).open {
        watch(app);
    }
}

/// Main window full screen, for the programme full screen in that window.
pub fn main_fullscreen(app: &AppHandle, url: &Url) {
    let on = url.query_pairs().any(|(k, v)| k == "on" && v == "1");
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_fullscreen(on);
    }
}

fn close_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

/// The two-second watch, while the output is wanted. One at a time.
fn watch(app: &AppHandle) {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while displays::load(&app).open {
            reconcile(&app);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        WATCHING.store(false, Ordering::SeqCst);
    });
}

/// Make the window match the choice and the displays connected now.
fn reconcile(app: &AppHandle) {
    let choice = displays::load(app);
    let present = choice.display.as_ref().and_then(|c| displays::connected(app).into_iter().find(|d| c.same(d)));
    let window = app.get_webview_window(LABEL);
    match (choice.open, present, window) {
        (true, Some(d), None) => {
            if let Err(e) = open_on(app, &d) {
                eprintln!("[desktop] the programme output could not open on {}: {e}", d.name);
            }
        }
        (true, None, Some(_)) => {
            eprintln!("[desktop] the programme output's display went away; its window is closed until it comes back");
            close_window(app);
        }
        _ => {}
    }
}

fn open_on(app: &AppHandle, d: &Display) -> tauri::Result<()> {
    let target = app.state::<Shell>().target.lock().unwrap().clone();
    let Some(target) = target else { return Ok(()) };
    let url = Url::parse(&programme_url(&target.base, &target.token)).map_err(|e| tauri::Error::Anyhow(e.into()))?;
    let handle = app.clone();
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(url))
        .title("GodwinMix programme")
        .user_agent(crate::USER_AGENT)
        .decorations(false)
        .visible(false)
        .focused(false)
        .on_navigation(move |url| on_navigation(&handle, url))
        .build()?;
    window.set_position(tauri::PhysicalPosition::new(d.x, d.y))?;
    window.set_size(tauri::PhysicalSize::new(d.width, d.height))?;
    window.set_fullscreen(true)?;
    window.show()?;
    Ok(())
}

/// The output page, with the token where the page looks for it: in the
/// fragment, which never reaches a server or a log.
pub fn programme_url(base: &str, token: &str) -> String {
    if token.is_empty() {
        format!("{base}/programme/")
    } else {
        format!("{base}/programme/#token={}", crate::core_link::urlencode(token))
    }
}

/// Esc on the output page asks to close it.
fn on_navigation(app: &AppHandle, url: &Url) -> bool {
    if url.scheme() != "godwinmix" {
        return true;
    }
    if url.host_str() == Some("programme-output-close") {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move { stop(&handle) });
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_page_carries_the_token_in_the_fragment() {
        assert_eq!(programme_url("http://127.0.0.1:5000", ""), "http://127.0.0.1:5000/programme/");
        assert_eq!(programme_url("http://127.0.0.1:5000", "a b"), "http://127.0.0.1:5000/programme/#token=a%20b");
    }
}
