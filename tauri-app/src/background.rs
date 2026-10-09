//! Running in the background, by choice, and saying so.
//!
//! Only `close` puts the app here, and only after a person pressed "Keep
//! running in the background". From launch on, the tray's tooltip, its icon
//! and the first lines of its menu say what the mixer is running, read every
//! few seconds from the public API, so the notification area is never the
//! only place something is streaming without a word about it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};

use crate::running_net::{self, Known};
use crate::Shell;

const EVERY: Duration = Duration::from_secs(5);

/// Whether the window was closed into the background.
static AWAY: AtomicBool = AtomicBool::new(false);
/// The lines the tray showed last, so the menu is rebuilt only on a change.
static SHOWN: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// The window goes, the tray stays and says what is running.
pub fn enter(app: &AppHandle) {
    AWAY.store(true, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    refresh(app.clone());
}

/// The window is back: tell the page, so it shows what is still running.
pub fn leave(app: &AppHandle) {
    if AWAY.swap(false, Ordering::SeqCst) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.eval("window.dispatchEvent(new Event('godwinmix-shown'))");
        }
    }
}

/// Keep the tray true for as long as the app runs.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            refresh(app.clone());
            tokio::time::sleep(EVERY).await;
        }
    });
}

async fn known(app: &AppHandle) -> Option<Known> {
    let target = app.state::<Shell>().target.lock().unwrap().clone()?;
    let http = app.state::<Shell>().http.clone();
    Some(running_net::read(&http, &target).await)
}

fn refresh(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let lines = known(&app).await.map(|k| k.lines()).unwrap_or_default();
        let away = AWAY.load(Ordering::SeqCst);
        let mut key = lines.clone();
        key.push(away.to_string());
        if SHOWN.lock().unwrap().as_ref() == Some(&key) {
            return;
        }
        *SHOWN.lock().unwrap() = Some(key);
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || crate::tray::paint(&app2, &lines, away));
    });
}

/// Stop all streaming, from the tray: one question, then every outgoing
/// thing stopped through the API, keys kept.
pub fn stop_all(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(known) = known(&app).await else { return };
        let things = known.outgoing();
        if things.is_empty() {
            return;
        }
        let list = things.iter().map(|t| format!("  {}", t.line)).collect::<Vec<_>>().join("\n");
        let body = format!("{list}\n\nViewers see the stream end. Each destination keeps its stream key.");
        let handle = app.clone();
        app.dialog()
            .message(body)
            .title("Stop all streaming?")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom("Stop all streaming".into(), "Keep streaming".into()))
            .show_with_result(move |answer| {
                let yes = matches!(&answer, MessageDialogResult::Ok)
                    || matches!(&answer, MessageDialogResult::Custom(s) if s == "Stop all streaming");
                if yes {
                    tauri::async_runtime::spawn(stop_now(handle, things));
                }
            });
    });
}

async fn stop_now(app: AppHandle, things: Vec<crate::running::Thing>) {
    let Some(target) = app.state::<Shell>().target.lock().unwrap().clone() else { return };
    let http = app.state::<Shell>().http.clone();
    let failed = running_net::stop_all(&http, &target, &things).await;
    if !failed.is_empty() {
        let body = format!("These did not stop:\n\n{}\n\nOpen GodwinMix and use What is running at the top of the window.", failed.join("\n"));
        crate::ui::tell(&app, "Not everything stopped", &body, MessageDialogKind::Error);
    }
    refresh(app);
}
