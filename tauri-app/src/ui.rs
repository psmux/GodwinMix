//! The window, the menu and the tray: everything the operator can click on
//! that belongs to the shell rather than to the mixer's own page.

use tauri::menu::{AboutMetadata, Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Url, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_window_state::{StateFlags, WindowExt};

use crate::Shell;

/// The exit status that tells `dev/desktop.sh` to stop the rest of the rig.
const EXIT_STOP_EVERYTHING: i32 = 2;

/// Seeded into every page the window loads, including the one the core
/// serves. It takes the token out of the address and puts it where the
/// operator UI looks for it, so the desktop app never asks a person to type a
/// token it generated itself, and the address bar is clean by the time the
/// page draws.
const SEED_TOKEN: &str = r#"
(function () {
  try {
    var url = new URL(window.location.href);
    var t = url.searchParams.get('token');
    if (!t) return;
    window.localStorage.setItem('gmx.token', t);
    url.searchParams.delete('token');
    window.history.replaceState(null, '', url.toString());
  } catch (e) {}
})();
"#;

/// Build the one window. It starts on the shell's own connect page, which
/// either sends it straight on to the core that was used last time or asks
/// which core to use.
pub fn build_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let config = app.config().app.windows[0].clone();
    let handle = app.clone();
    let window = WebviewWindowBuilder::from_config(app, &config)?
        .user_agent(crate::USER_AGENT)
        .initialization_script(SEED_TOKEN)
        .on_navigation(move |url| on_navigation(&handle, url))
        .build()?;
    // Where the shell's own page lives, kept so that Connect can come back to
    // it from whatever the core was showing.
    if let Ok(url) = window.url() {
        *app.state::<Shell>().app_url.lock().unwrap() = Some(url.to_string());
    }
    let _ = window.restore_state(StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED);
    Ok(window)
}

/// The one channel a page has into this shell: navigating to a scheme the
/// shell answers. `liveboxmix://` is answered too, until 0.3, because a page
/// cached from before the rename has a quit button pointed at it.
fn on_navigation(app: &AppHandle, url: &Url) -> bool {
    if !matches!(url.scheme(), "godwinmix" | "liveboxmix") {
        return true;
    }
    match url.host_str().unwrap_or("") {
        "quit" => crate::quit(app, false),
        "quit-all" => crate::quit(app, true),
        _ => {}
    }
    false
}

/// The application menu. On macOS the first submenu is the application menu,
/// which is where About and Quit belong.
pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let connect = MenuItemBuilder::with_id("connect", "Connect to a mixer...")
        .accelerator("CmdOrCtrl+Shift+C")
        .build(app)?;
    let logs = MenuItemBuilder::with_id("logs", "Open logs folder").build(app)?;
    let config = MenuItemBuilder::with_id("config", "Open config folder").build(app)?;
    let updates = MenuItemBuilder::with_id("updates", "Check for updates...").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").accelerator("CmdOrCtrl+Q").build(app)?;
    let quit_all = MenuItemBuilder::with_id("quit-all", "Quit and stop the mixer")
        .accelerator("CmdOrCtrl+Shift+Q")
        .build(app)?;

    let about = PredefinedMenuItem::about(app, Some("About GodwinMix"), Some(about_metadata()))?;
    let separator = || PredefinedMenuItem::separator(app);
    let app_menu = SubmenuBuilder::new(app, "GodwinMix")
        .items(&[&about, &updates, &separator()?, &connect, &separator()?, &logs, &config, &separator()?, &quit, &quit_all])
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .items(&[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &separator()?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ])
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .items(&[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ])
        .build()?;
    MenuBuilder::new(app).items(&[&app_menu, &edit, &window]).build()
}

fn about_metadata() -> AboutMetadata<'static> {
    AboutMetadata {
        name: Some("GodwinMix".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        comments: Some(
            "A live video mixer. This window is a shell: the operator UI comes from the mixer it is connected to, so a mixer on a server looks the same as one on this computer.".into(),
        ),
        website: Some("https://github.com/godwin/godwinmix".into()),
        ..Default::default()
    }
}

/// The tray icon: Show, Connect, Quit. It is what gets the window back after
/// someone closes it, which on macOS leaves the app running.
pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id("show", "Show GodwinMix").build(app)?;
    let connect = MenuItemBuilder::with_id("connect", "Connect to a mixer...").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&show, &connect, &quit]).build()?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("GodwinMix")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()));
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

/// One handler for the menu bar and the tray, since the items mean the same
/// thing in both.
pub fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "show" => show(app),
        "connect" => connect(app),
        "logs" => reveal(app, crate::settings::log_dir(app).ok()),
        "config" => reveal(app, crate::settings::data_dir(app).ok()),
        "updates" => check_for_updates(app.clone()),
        "quit" => crate::quit(app, false),
        "quit-all" => crate::quit(app, true),
        _ => {}
    }
}

/// Bring the window back, or make it again if it was closed.
pub fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    if let Err(e) = build_window(app) {
        eprintln!("[desktop] could not open the window again: {e}");
    }
}

/// Send the window back to the connect page. The fragment carries a number
/// that changes every time so that asking twice in a row really does reload
/// the page rather than sitting on the same address.
pub fn connect(app: &AppHandle) {
    show(app);
    let Some(window) = app.get_webview_window("main") else { return };
    let base = app.state::<Shell>().app_url.lock().unwrap().clone();
    let Some(base) = base else { return };
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let Ok(mut url) = Url::parse(&base) else { return };
    url.set_fragment(Some(&format!("connect-{nonce}")));
    if let Err(e) = window.navigate(url) {
        eprintln!("[desktop] could not open the connect page: {e}");
    }
}

fn reveal(app: &AppHandle, dir: Option<std::path::PathBuf>) {
    let Some(dir) = dir else {
        tell(app, "No folder yet", "This folder appears the first time the app starts a mixer.", MessageDialogKind::Info);
        return;
    };
    if let Err(e) = app.opener().open_path(dir.display().to_string(), None::<&str>) {
        tell(app, "Could not open the folder", &format!("{}\n{e}", dir.display()), MessageDialogKind::Error);
    }
}

/// Check the update endpoint and say what came back. The endpoint in
/// `tauri.conf.json` is a placeholder until releases are published and signed,
/// so "not configured yet" is the honest answer and the one an operator gets.
fn check_for_updates(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        use tauri_plugin_updater::UpdaterExt;
        let outcome = match app.updater() {
            Ok(updater) => updater.check().await,
            Err(e) => Err(e),
        };
        match outcome {
            Ok(Some(update)) => tell(
                &app,
                "An update is available",
                &format!(
                    "GodwinMix {} is out. This copy is {}. Download it from the releases page.",
                    update.version, update.current_version
                ),
                MessageDialogKind::Info,
            ),
            Ok(None) => tell(&app, "Up to date", &format!("This is GodwinMix {}.", env!("CARGO_PKG_VERSION")), MessageDialogKind::Info),
            Err(e) => tell(
                &app,
                "Could not check for updates",
                &format!("{e}\n\nAutomatic updates are not switched on in this build. See docs/how-to/desktop-app.md."),
                MessageDialogKind::Warning,
            ),
        }
    });
}

/// A message box. Not blocking: the window may be showing a page served by
/// the core, and the shell must not stop that page drawing to say something.
pub fn tell(app: &AppHandle, title: &str, body: &str, kind: MessageDialogKind) {
    app.dialog().message(body).title(title).kind(kind).show(|_| {});
}

/// What the title bar says while connected, so that a second window on a
/// second machine is never mistaken for this one.
pub fn set_title(app: &AppHandle, info: &crate::core_link::CoreInfo) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title(&format!("GodwinMix {} on {}", info.version, info.label));
    }
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&format!("GodwinMix {} on {}", info.version, info.label)));
    }
}

pub const EXIT_ALL: i32 = EXIT_STOP_EVERYTHING;
