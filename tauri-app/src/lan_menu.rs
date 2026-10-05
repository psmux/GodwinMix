//! The menu item for "Let other devices on this network connect", and the
//! question the app asks before it restarts the mixer to change it. The
//! setting itself is in `lan`.

use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder};
use tauri::{AppHandle, Wry};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::lan::{load, save, Lan};

pub const MENU_ID: &str = "lan";
const MENU_TITLE: &str = "Let other devices on this network connect";

/// The menu item as last built. The menu is built again whenever a page puts
/// its own menus up, and only the newest item is on screen.
static ITEM: Mutex<Option<CheckMenuItem<Wry>>> = Mutex::new(None);

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
