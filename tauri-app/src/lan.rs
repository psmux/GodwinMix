//! "Let other devices on this network connect": whether the mixer on this
//! computer answers on loopback only, which is the default, or on every
//! network the computer is on, so a phone can run the show.
//!
//! On, the mixer binds `0.0.0.0` on a port chosen once and kept in
//! `lan.json`, so the link a phone saved last week still works today. Its
//! certificate then names the computer's LAN addresses and `.local` name as
//! well as localhost. It is never an open port: the mixer is always started
//! with the generated token, and a phone signs in with a device token from
//! Help > Open on another device. Off, it is loopback on a fresh port at
//! every start, as before.
//!
//! Changing it restarts the mixer, after asking, because the bind address is
//! fixed for the life of the process.

use std::net::TcpListener;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder};
use tauri::{AppHandle, Wry};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

const LAN_FILE: &str = "lan.json";
pub const MENU_ID: &str = "lan";
const MENU_TITLE: &str = "Let other devices on this network connect";

/// The setting as it is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lan {
    #[serde(default)]
    pub enabled: bool,
    /// The port the mixer keeps while this is on. Chosen the first time it
    /// is turned on, and kept after it is turned off so turning it on again
    /// brings back the same address.
    #[serde(default)]
    pub port: Option<u16>,
}

/// The menu item as last built. The menu is built again whenever a page puts
/// its own menus up, and only the newest item is on screen.
static ITEM: Mutex<Option<CheckMenuItem<Wry>>> = Mutex::new(None);

pub fn load(app: &AppHandle) -> Lan {
    let Ok(dir) = crate::settings::data_dir(app) else { return Lan::default() };
    std::fs::read_to_string(dir.join(LAN_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, lan: Lan) -> Result<(), String> {
    let dir = crate::settings::data_dir(app).map_err(|e| format!("no data folder to keep the setting in: {e}"))?;
    let text = serde_json::to_string_pretty(&lan).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(LAN_FILE), text).map_err(|e| format!("could not keep the setting: {e}"))
}

/// The address the mixer binds.
pub fn bind_host(app: &AppHandle) -> &'static str {
    if load(app).enabled {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    }
}

/// The kept port when the setting is on, waiting a moment for a mixer that
/// was just stopped to let go of it. `None` when the setting is off, and the
/// caller takes a fresh port as before.
pub async fn kept_port(app: &AppHandle) -> Option<u16> {
    let mut lan = load(app);
    if !lan.enabled {
        return None;
    }
    if let Some(port) = lan.port {
        for _ in 0..15 {
            if TcpListener::bind(("0.0.0.0", port)).is_ok() {
                return Some(port);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        eprintln!("[desktop] port {port} is taken by something else; the mixer moves to a new one and phones need a new code");
    }
    let fresh = TcpListener::bind(("0.0.0.0", 0)).ok()?.local_addr().ok()?.port();
    lan.port = Some(fresh);
    if let Err(why) = save(app, lan) {
        eprintln!("[desktop] {why}");
    }
    Some(fresh)
}

/// The check item for the app's own menu.
pub fn menu_item(app: &AppHandle) -> tauri::Result<CheckMenuItem<Wry>> {
    let item = CheckMenuItemBuilder::with_id(MENU_ID, MENU_TITLE).checked(load(app).enabled).build(app)?;
    *ITEM.lock().unwrap() = Some(item.clone());
    Ok(item)
}

fn show_checked(on: bool) {
    if let Some(item) = ITEM.lock().unwrap().as_ref() {
        let _ = item.set_checked(on);
    }
}

/// The menu item was clicked: say what changes, and on yes, change it and
/// restart the mixer. Nothing changes on no.
pub fn toggle(app: &AppHandle) {
    let lan = load(app);
    let on = !lan.enabled;
    // The platform may already have flipped the tick; it shows the setting
    // until the operator has said yes.
    show_checked(lan.enabled);
    let (title, body) = if on {
        (
            "Let other devices connect?",
            "The mixer will answer on this computer's network addresses as well, on a port it keeps \
             from now on, so phones and tablets on the same network can open it. Every device still \
             needs a token: Help > Open on another device makes one per device as a QR code.\n\n\
             The mixer restarts to do this. The programme and every output stop for a few seconds.",
        )
    } else {
        (
            "Stop other devices connecting?",
            "The mixer will answer on this computer only. Phones and tablets that are connected lose \
             it. Their device tokens are kept, so turning this on again lets them back in.\n\n\
             The mixer restarts to do this. The programme and every output stop for a few seconds.",
        )
    };
    let handle = app.clone();
    app.dialog()
        .message(body)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Restart the mixer".into(), "Cancel".into()))
        .show(move |yes| {
            if yes {
                apply(&handle, Lan { enabled: on, ..lan });
            }
        });
}

fn apply(app: &AppHandle, lan: Lan) {
    if let Err(why) = save(app, lan) {
        crate::ui::tell(app, "The setting was not changed", &why, MessageDialogKind::Error);
        return;
    }
    show_checked(lan.enabled);
    crate::restart::after_setting_change(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_is_off_until_turned_on_and_an_old_file_still_reads() {
        assert!(!Lan::default().enabled);
        let back: Lan = serde_json::from_str("{}").unwrap();
        assert_eq!(back, Lan::default());
        let on: Lan = serde_json::from_str(r#"{"enabled":true,"port":51234}"#).unwrap();
        assert_eq!(on.port, Some(51234));
    }
}
