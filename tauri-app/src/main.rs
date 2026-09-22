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
//   * the camera, the screen and the microphone, which are plugins, put where
//     the mixer will find them before it starts;
//   * a connect dialog, so the same app drives a headless server;
//   * a tray icon, a menu, remembered window geometry, one instance;
//   * two ways out, as menu items and as `godwinmix://quit` and
//     `godwinmix://quit-all` navigations from the page;
//   * a restart of the mixer on this computer, from the menu, from a
//     `godwinmix://restart` navigation, and whenever the mixer exits asking
//     for one (`core.restart`).
//
// There is no address and no token compiled into this program. The port is
// taken from the operating system at every start and the token is generated
// into the application data directory on first run.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod core_link;
mod plugins;
mod restart;
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
        .invoke_handler(tauri::generate_handler![
            commands::saved_connection,
            commands::connect_core,
            restart::restart_core
        ])
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
            // Any other route to the door: the last window closing on
            // Windows and Linux, or the operating system asking, which is
            // what macOS does for Quit in the Dock and for an Apple Event.
            // Only when a mixer of ours is running: with nothing to wind
            // down, the exit should simply happen.
            RunEvent::ExitRequested { api, .. }
                if app.state::<Shell>().local.lock().unwrap().is_some() =>
            {
                api.prevent_exit();
                quit(app, false);
            }
            // The door itself, which on macOS is where Quit in the Dock and
            // a quit Apple Event arrive: no ExitRequested first, and the run
            // loop already winding down. Blocking the main thread here is
            // right, because the alternative is the process leaving before
            // the daemon has closed its outputs. Every wait inside is
            // bounded, and the daemon is killed if it overruns them.
            RunEvent::Exit => {
                let local = app.state::<Shell>().local.lock().unwrap().take();
                if let Some(local) = local {
                    let app = app.clone();
                    tauri::async_runtime::block_on(sidecar::stop(&app, local));
                }
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

/// Prove the GStreamer inside the app is the one that gets loaded.
///
/// `None` when this build carries no runtime, which is how a developer build
/// and the Linux .deb are meant to work: they use the GStreamer on the
/// machine and there is nothing here to check. `Some(0)` when the bundled
/// runtime answered for every element the mixer cannot run without, out of
/// files inside the bundle. `Some(1)` when it did not.
///
/// The four elements are the ones a mix and a stream cannot be built without:
/// the compositor is the canvas, a software H.264 encoder is what a machine
/// with no GPU uses, and `rtmp2sink` and `srtsink` are the two ways the
/// programme leaves the building.
fn bundled_runtime_check(app: &AppHandle) -> Option<i32> {
    let root = sidecar::bundled_gstreamer(app)?;
    println!("bundled GStreamer in {}", root.display());

    let mut failed = false;
    // The same list the trim script checks: the slots' flip and crop and
    // the mix beside the two outputs, because a runtime that has the
    // encoder and not `videoflip` builds no programme at all, which is what
    // the first bundled app on a macOS runner found.
    for element in ["compositor", "videoflip", "videocrop", "videoscale", "audiomixer", "proxysink", "rtmp2sink", "srtsink"] {
        match sidecar::inspect_element(app, element) {
            Ok(file) => println!("  {element} from {}", file.display()),
            Err(why) => {
                println!("FAIL {why}");
                failed = true;
            }
        }
    }
    // The catalogue carries two software H.264 entries and either one is
    // enough. openh264 is the one with a licence that can be redistributed;
    // x264 is the one most runtimes ship.
    let software = ["openh264enc", "x264enc"]
        .into_iter()
        .find_map(|e| sidecar::inspect_element(app, e).ok().map(|f| (e, f)));
    match software {
        Some((element, file)) => println!("  {element} from {}", file.display()),
        None => {
            println!("FAIL neither openh264enc nor x264enc is in the bundled runtime");
            failed = true;
        }
    }
    if failed {
        println!("the bundled runtime is not usable; rebuild it with dev/bundle-gstreamer.sh");
        return Some(1);
    }
    println!("the bundled runtime answered for every element, and none came from a system install");
    Some(0)
}

/// Prove the mixer loaded the device plugins the app carries.
///
/// `None` when this build carries none, the same way the runtime check is
/// skipped by a build with no GStreamer in it. Otherwise every plugin staged
/// in the bundle has to come back from `/api/v1/plugins` at the version the
/// bundle carries and with nothing wrong with it, because "it is in the
/// resources directory" and "the mixer loaded it" are different claims and
/// only the second one gives an operator a camera.
///
/// The list is read off the bundle rather than written down here. A fourth
/// plugin added to `dev/bundle-plugins.sh` is then checked by this without
/// anybody remembering to come back and add it.
async fn bundled_plugins_check(app: &AppHandle, target: &Target) -> Option<i32> {
    let root = plugins::bundled(app)?;
    let carried = plugins::versions(&root);
    println!("{} device plugins in {}", carried.len(), root.display());

    let http = app.state::<Shell>().http.clone();
    let installed = match core_link::plugins(&http, target).await {
        Ok(installed) => installed,
        Err(why) => {
            println!("FAIL the mixer would not say what it has installed: {why}");
            return Some(1);
        }
    };

    let mut failed = false;
    for (name, version, _) in carried {
        match installed.iter().find(|(got, _, _)| *got == name) {
            Some((_, got, None)) if *got == version => println!("  {name} {version} loaded"),
            Some((_, got, None)) => {
                println!("FAIL the mixer loaded {name} {got}, and the app carries {version}");
                failed = true;
            }
            Some((_, _, Some(problem))) => {
                println!("FAIL the mixer will not load {name}: {problem}");
                failed = true;
            }
            None => {
                println!("FAIL the mixer did not find {name} at all");
                failed = true;
            }
        }
    }
    if failed {
        println!("the plugins in the app are not the ones the mixer has; rebuild them with dev/bundle-plugins.sh");
        return Some(1);
    }
    println!("every device plugin the app carries is loaded and has no problem");
    Some(0)
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

    if let Some(code) = bundled_runtime_check(app) {
        if code != 0 {
            return code;
        }
    }

    let local = match sidecar::ensure(app).await {
        Ok(local) => local,
        Err(why) => {
            println!("FAIL the mixer did not start: {why}");
            return 1;
        }
    };
    let target = local.target.clone();
    println!("started on {} with a {} character token", target.base, target.token.len());
    match local.pid() {
        Some(pid) => println!("its process id is {pid}"),
        None => println!("it was already running from an earlier run"),
    }
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

    if let Some(code) = bundled_plugins_check(app, &target).await {
        if code != 0 {
            sidecar::stop(app, local).await;
            return code;
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
