// LiveboxMix desktop shell.
//
// Deliberately thin. The operator UI is a web app served by the mixer daemon,
// so this is a native window pointed at it. Running the same UI locally and
// remotely means there is only one implementation to keep in step: a remote
// operator gets the identical page by changing the address.
//
// The one thing the shell adds is a way out. Closing the window, or Quit,
// leaves the mixer running: the stream is not on this window, and an operator
// who closes it by accident must not take the programme down with it. "Quit
// and stop the mixer" is the deliberate version: it asks the mixer to shut
// down over its API and then exits with status 2, which dev/desktop.sh reads
// as "stop the rest of the rig too".
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

/// Where the mixer is. The same address the window is pointed at.
const MIXER: &str = "127.0.0.1:8080";

/// The exit status that tells the launcher to stop the whole rig.
const EXIT_STOP_EVERYTHING: i32 = 2;

/// Ask the mixer to shut down. Plain HTTP over a socket rather than an HTTP
/// crate: it is one request, and the shell has no other reason to carry one.
fn stop_mixer() {
    let Ok(mut s) = TcpStream::connect_timeout(&MIXER.parse().unwrap(), Duration::from_secs(2)) else {
        eprintln!("[desktop] no mixer at {MIXER} to stop");
        return;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    let req = format!(
        "POST /api/shutdown HTTP/1.1\r\nHost: {MIXER}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    if s.write_all(req.as_bytes()).is_ok() {
        let mut reply = String::new();
        let _ = s.read_to_string(&mut reply);
        eprintln!("[desktop] mixer said: {}", reply.lines().next().unwrap_or(""));
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let quit = MenuItemBuilder::with_id("quit", "Quit (leave the mixer running)")
                .accelerator("CmdOrCtrl+Q")
                .build(app)?;
            let quit_all = MenuItemBuilder::with_id("quit-all", "Quit and stop the mixer")
                .accelerator("CmdOrCtrl+Shift+Q")
                .build(app)?;
            // The first submenu is the application menu on macOS.
            let app_menu = SubmenuBuilder::new(app, "LiveboxMix")
                .item(&quit)
                .item(&quit_all)
                .build()?;
            let edit = SubmenuBuilder::new(app, "Edit")
                .items(&[
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ])
                .build()?;
            let menu = MenuBuilder::new(app).items(&[&app_menu, &edit]).build()?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, event| match event.id().as_ref() {
                "quit" => app.exit(0),
                "quit-all" => {
                    stop_mixer();
                    // Not `app.exit(code)`: that winds the run loop down and
                    // the process still returns 0, so the launcher never saw
                    // the status it waits for. A shell this thin has nothing
                    // to tidy; leave at once with the status that means it.
                    std::process::exit(EXIT_STOP_EVERYTHING);
                }
                _ => {}
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start the LiveboxMix desktop shell");
}
