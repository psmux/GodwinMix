// GodwinMix desktop shell.
//
// Deliberately thin. The operator UI is a web app served by the mixer, so
// this is a native window pointed at one. The same window shows a mixer
// running on this computer and a mixer running on a server in another
// building, which is why there is only one UI to keep in step.
//
// What the shell adds to a browser tab:
//
//   * the mixer itself, bundled as a sidecar, started on a free port with a
//     generated token and stopped again when the app quits;
//   * a connect dialog, so the same app drives a headless server;
//   * a tray icon, a menu, remembered window geometry, one instance;
//   * two ways out, as menu items and as `godwinmix://quit` and
//     `godwinmix://quit-all` navigations from the page.
//
// There is no address and no token compiled into this program. The port is
// taken from the operating system at every start and the token is generated
// into the application data directory on first run.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod core_link;
mod settings;
mod sidecar;
mod ui;

use std::sync::Mutex;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

use core_link::Target;
use sidecar::Local;

/// What the page sees in `navigator.userAgent`, so the UI can show the two
/// exit buttons only when it is running in this window and not in a browser.
/// The page matches on the name, so the version after it is free to change.
pub const USER_AGENT: &str = concat!("GodwinMix-Desktop/", env!("CARGO_PKG_VERSION"));

/// Everything the shell holds while it runs.
#[derive(Default)]
pub struct Shell {
    /// One HTTP client for the two requests the shell makes.
    pub http: reqwest::Client,
    /// The core the window is pointed at, once a connection has been made.
    pub target: Mutex<Option<Target>>,
    /// The mixer this app started, when it started one.
    pub local: Mutex<Option<Local>>,
    /// Where the shell's own connect page lives, so the menu can go back to
    /// it from whatever the core is showing.
    pub app_url: Mutex<Option<String>>,
}

fn main() {
    let headless = std::env::args().any(|a| a == "--headless-check");

    tauri::Builder::default()
        // First, as the plugin's own documentation insists: a second launch
        // has to reach the copy already running before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| ui::show(app)))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(Shell { http: core_link::client(), ..Shell::default() })
        .invoke_handler(tauri::generate_handler![commands::saved_connection, commands::connect_core])
        .setup(move |app| {
            let handle = app.handle().clone();
            if headless {
                // No window, no menu, no tray: start the mixer the way the
                // app would, prove it answers, stop it, prove it stopped.
                tauri::async_runtime::spawn(async move {
                    std::process::exit(headless_check(&handle).await);
                });
                return Ok(());
            }
            let menu = ui::build_menu(&handle)?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, event| ui::on_menu(app, event.id().as_ref()));
            ui::build_tray(&handle)?;
            ui::build_window(&handle)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start the GodwinMix desktop shell")
        .run(|app, event| match event {
            // Closing the window hides it rather than ending the app. A mixer
            // that stopped because someone tidied their desktop would be a
            // mixer that took the programme off air; the tray icon and Show
            // bring it back, and Quit is the way out.
            RunEvent::WindowEvent { label, event: WindowEvent::CloseRequested { api, .. }, .. } => {
                if label == "main" {
                    api.prevent_close();
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }
            // Any other route to the door, the operating system's included.
            // Only when a mixer of ours is still running: otherwise there is
            // nothing to wind down and the exit should just happen.
            RunEvent::ExitRequested { api, code: None, .. }
                if app.state::<Shell>().local.lock().unwrap().is_some() =>
            {
                api.prevent_exit();
                quit(app, false);
            }
            _ => {}
        });
}

/// Leave, stopping what this app started. With `stop_core`, the mixer it is
/// connected to is asked to stop as well, even a remote one, and the exit
/// status is 2: that is what `dev/desktop.sh` reads as "stop the rest of the
/// rig too".
pub fn quit(app: &AppHandle, stop_core: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let code = wind_down(&app, stop_core).await;
        // Window geometry is saved by the plugin when the run loop ends, and
        // the run loop is not going to end: this leaves at once instead.
        let _ = app.save_window_state(StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED);
        std::process::exit(code);
    });
}

/// Stop the mixer this app started, and the one it is connected to if asked.
async fn wind_down(app: &AppHandle, stop_core: bool) -> i32 {
    let (local, target) = {
        let shell = app.state::<Shell>();
        let local = shell.local.lock().unwrap().take();
        let target = shell.target.lock().unwrap().clone();
        (local, target)
    };
    if stop_core {
        // A remote core is stopped over the API and nothing else; a local one
        // is stopped below, which asks over the API first anyway, so it is
        // not asked twice.
        let ours = local.as_ref().map(|l| l.target.base.clone());
        if let Some(target) = target.filter(|t| Some(&t.base) != ours.as_ref()) {
            let http = app.state::<Shell>().http.clone();
            if let Err(e) = core_link::shutdown(&http, &target).await {
                eprintln!("[desktop] {e}");
            }
        }
    }
    if let Some(local) = local {
        sidecar::stop(app, local).await;
    }
    if stop_core {
        ui::EXIT_ALL
    } else {
        0
    }
}

/// `--headless-check`: the acceptance test for the sidecar, runnable on a
/// machine with no one at the keyboard and in CI.
///
/// It starts the mixer exactly as the app does, asks it what it is, stops it,
/// and checks the port went quiet. Prints what it found, and returns 0 only
/// if every step held.
async fn headless_check(app: &AppHandle) -> i32 {
    let data = settings::data_dir(app).map(|d| d.display().to_string()).unwrap_or_default();
    let logs = settings::log_dir(app).map(|d| d.display().to_string()).unwrap_or_default();
    println!("config and token in {data}");
    println!("logs in {logs}");

    let local = match sidecar::start(app).await {
        Ok(local) => local,
        Err(why) => {
            println!("FAIL the mixer did not start: {why}");
            return 1;
        }
    };
    let target = local.target.clone();
    println!("started on {} with a {} character token", target.base, target.token.len());
    println!("its process id is {}", local.pid());
    if target.base.ends_with(":8080") {
        println!("FAIL the port was 8080, which means it was not taken from the operating system");
        return 1;
    }
    if target.token.len() < 32 {
        println!("FAIL the token is too short to be generated");
        return 1;
    }

    let http = app.state::<Shell>().http.clone();
    match core_link::info(&http, &target, "this computer").await {
        Ok(info) => println!("it says it is {} {}", info.name, info.version),
        Err(why) => {
            println!("FAIL it did not say what it is: {why}");
            return 1;
        }
    }

    // Long enough to prove it is not a process that starts and falls over,
    // and long enough for someone watching the process list to see it there.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    if !local.is_running() {
        println!("FAIL it did not stay up for three seconds");
        return 1;
    }
    println!("still up after three seconds");

    sidecar::stop(app, local).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    match core_link::info(&http, &target, "this computer").await {
        Ok(_) => {
            println!("FAIL {} still answers after the app stopped it", target.base);
            1
        }
        Err(_) => {
            println!("stopped: {} no longer answers", target.base);
            println!("OK");
            0
        }
    }
}
