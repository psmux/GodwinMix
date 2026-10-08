//! Restarting the mixer on this computer.
//!
//! Three ways in, one way through. The menu's "Restart the mixer" and a
//! `godwinmix://restart` navigation from the page stop the mixer and start it
//! again. The mixer can also ask for it itself: `core.restart` on a mixer
//! started with `--supervised` exits with [`RESTART_EXIT_CODE`], and `record`
//! in the sidecar module sends that here. That is how a page, an agent or a
//! browser on another machine restarts the desktop's mixer over the protocol
//! with no channel into this shell at all.
//!
//! The new mixer is given the old one's port and the same token, so the page
//! already open on it reconnects by itself. Only when that port cannot be had
//! is it started on a fresh one, and then the window is sent there.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, Url};
use tauri_plugin_dialog::MessageDialogKind;

use crate::core_link::{self, CoreInfo};
use crate::{sidecar, Shell};

/// The status the mixer exits with when `core.restart` asked it to. The same
/// number as `RESTART_EXIT_CODE` in the daemon's `control::methods::lifecycle`;
/// this shell does not link the daemon, so it is written twice.
pub const RESTART_EXIT_CODE: i32 = 75;

/// The statuses `network.share` exits with: start the mixer again for other
/// devices, or for this computer only. The same numbers as `SHARE_EXIT_CODE`
/// and `LOCAL_EXIT_CODE` in the daemon's `control::methods::network`.
pub const SHARE_EXIT_CODE: i32 = 76;
pub const LOCAL_EXIT_CODE: i32 = 77;

/// One restart at a time: a menu click during a restart the mixer asked for
/// must not start a second mixer beside the first.
static RESTARTING: AtomicBool = AtomicBool::new(false);

/// For the shell's own page. The page the core serves has no IPC (see
/// `capabilities/desktop-shell.json`); it navigates to `godwinmix://restart`,
/// or calls `core.restart`, which comes back here through the exit status.
#[tauri::command]
pub async fn restart_core(app: AppHandle) -> Result<CoreInfo, String> {
    restart(&app).await
}

/// The menu item and the `godwinmix://restart` navigation.
pub fn from_page(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { report(&app, restart(&app).await) });
}

/// "Let other devices on this network connect" changed. The address the
/// mixer binds is fixed for its life, so it is started again, on the kept
/// port when the setting is now on and on a fresh one when it is off.
pub fn after_setting_change(app: &AppHandle) {
    if app.state::<Shell>().local.lock().unwrap().is_none() {
        // The window is on a mixer elsewhere, or on none. The setting is
        // kept, and the next mixer this app starts here uses it.
        crate::ui::tell(
            app,
            "Saved",
            "No mixer is running on this computer, so the setting applies the next time this app starts one.",
            MessageDialogKind::Info,
        );
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = restart_with(&app, false).await;
        report(&app, outcome);
    });
}

/// The mixer exited asking to be started again.
pub fn after_exit(app: AppHandle) {
    tauri::async_runtime::spawn(async move { report(&app, restart(&app).await) });
}

/// The mixer exited because a page asked, with `network.share`, for other
/// devices to be let in or kept out: from this window, a browser or a phone.
/// The choice is kept as the menu item keeps it, and on the port the mixer is
/// on now, so the page that asked reconnects where it is.
pub fn after_network_exit(app: AppHandle, enabled: bool) {
    let port = app.state::<Shell>().local.lock().unwrap().as_ref().map(|l| l.port);
    let mut lan = crate::lan::load(&app);
    lan.enabled = enabled;
    if enabled && port.is_some() {
        lan.port = port;
    }
    if let Err(why) = crate::lan::save(&app, lan) {
        report(&app, Err(why));
        return;
    }
    crate::lan_menu::show_checked(enabled);
    tauri::async_runtime::spawn(async move { report(&app, restart(&app).await) });
}

fn report(app: &AppHandle, outcome: Result<CoreInfo, String>) {
    if let Err(why) = outcome {
        eprintln!("[desktop] restart: {why}");
        crate::ui::tell(app, "The mixer did not restart", &why, MessageDialogKind::Error);
    }
}

async fn restart(app: &AppHandle) -> Result<CoreInfo, String> {
    restart_with(app, true).await
}

async fn restart_with(app: &AppHandle, same_port: bool) -> Result<CoreInfo, String> {
    if RESTARTING.swap(true, Ordering::SeqCst) {
        return Err("The mixer is already restarting. Wait a few seconds for it to come back.".into());
    }
    let outcome = restart_once(app, same_port).await;
    RESTARTING.store(false, Ordering::SeqCst);
    outcome
}

async fn restart_once(app: &AppHandle, same_port: bool) -> Result<CoreInfo, String> {
    let old = app.state::<Shell>().local.lock().unwrap().take();
    let Some(old) = old else {
        return Err("This app has not started a mixer on this computer, so there is none for it \
                    to restart. A mixer on another machine restarts from its own page, or with \
                    core.restart."
            .into());
    };
    let (port, old_base) = (old.port, old.target.base.clone());
    sidecar::stop(app, old).await;
    let first = if same_port { sidecar::start_on(app, port).await } else { sidecar::start(app).await };
    let fresh = match first {
        Ok(local) => local,
        Err(first) => sidecar::start(app)
            .await
            .map_err(|e| format!("{first}\nTried again on another port: {e}"))?,
    };
    let target = fresh.target.clone();
    *app.state::<Shell>().local.lock().unwrap() = Some(fresh);
    let http = app.state::<Shell>().http.clone();
    let info = core_link::info(&http, &target, "this computer").await?;
    follow(app, &old_base, target, &info);
    Ok(info)
}

/// Point the window at the new mixer when it was looking at the old one and
/// the address changed. On the same port the page reconnects by itself.
fn follow(app: &AppHandle, old_base: &str, target: core_link::Target, info: &CoreInfo) {
    let shell = app.state::<Shell>();
    let mut current = shell.target.lock().unwrap();
    let watching_ours = current.as_ref().is_some_and(|t| t.base == old_base);
    if !watching_ours {
        return;
    }
    let moved = target.base != old_base;
    *current = Some(target);
    drop(current);
    if moved {
        if let (Some(window), Ok(url)) = (app.get_webview_window("main"), Url::parse(&info.url)) {
            let _ = window.navigate(url);
        }
    }
}
