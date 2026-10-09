//! What closing the window, Quit in the tray and Quit in the menu do.
//!
//! Until 0.3 closing the window hid it, and the mixer carried on with every
//! stream and recording it had, with nothing on screen saying so. A tester
//! closed the app while streaming to YouTube and the stream ran on for hours.
//! Now the rule is: with nothing running, the app quits, mixer and all. With
//! anything running it asks, in words, and offers three answers: stop
//! everything and quit, keep running in the background, or cancel. Nothing
//! stays running without that choice, and nothing live is stopped without it.
//!
//! The decision is a pure function (`decide`, `choice`) so it is tested the
//! way `revive` is; the rest is the dialog and the three actions.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};

use crate::running_net::Known;
use crate::Shell;

pub const STOP_AND_QUIT: &str = "Stop everything and quit";
pub const BACKGROUND: &str = "Keep running in the background";
pub const LEAVE: &str = "Quit and leave it streaming";
pub const CANCEL: &str = "Cancel";

/// One question at a time: a second Quit while the first is being asked is
/// the same request.
static ASKING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, PartialEq, Eq)]
pub enum Closing {
    /// Nothing is running: go, and take the mixer with you.
    Quit,
    /// The mixer on this computer is running something: ask the three way
    /// question.
    AskLocal,
    /// A mixer on another machine is running something and this app is only
    /// its window: quitting leaves it running there, which is said first.
    AskRemote,
    /// "Quit and stop the mixer" on a mixer on another machine that is
    /// running something: stopping it is asked about first.
    AskStop,
}

/// What to do about a request to close. `local` is whether the mixer is the
/// one this app started; `stop_core` is "Quit and stop the mixer".
pub fn decide(known: Option<&Known>, local: bool, stop_core: bool) -> Closing {
    match known {
        None => Closing::Quit,
        Some(Known::Things(things)) if things.is_empty() => Closing::Quit,
        Some(_) if local => Closing::AskLocal,
        Some(_) if stop_core => Closing::AskStop,
        Some(_) => Closing::AskRemote,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Choice {
    StopAndQuit,
    Background,
    /// Quit and leave a remote mixer as it is.
    Leave,
    Cancel,
}

/// Which button was pressed. A platform hands back either the label or the
/// plain Yes, No or Cancel it maps the buttons to.
pub fn choice(result: &MessageDialogResult, local: bool) -> Choice {
    match result {
        MessageDialogResult::Custom(label) if label == STOP_AND_QUIT => Choice::StopAndQuit,
        MessageDialogResult::Custom(label) if label == BACKGROUND => Choice::Background,
        MessageDialogResult::Custom(label) if label == LEAVE => Choice::Leave,
        MessageDialogResult::Yes => Choice::StopAndQuit,
        MessageDialogResult::Ok if local => Choice::StopAndQuit,
        MessageDialogResult::Ok => Choice::Leave,
        MessageDialogResult::No => Choice::Background,
        _ => Choice::Cancel,
    }
}

/// The question, in words, naming what is running.
pub fn question(known: &Known, closing: &Closing, server: &str) -> String {
    let what = match known {
        Known::Things(things) => things.iter().map(|t| format!("  {}", t.line)).collect::<Vec<_>>().join("\n"),
        Known::Unknown(why) => format!("  The mixer did not say what it is running ({why}), so it may still be streaming."),
    };
    if *closing == Closing::AskStop {
        return format!("The mixer at {server} is still running:

{what}

Stop everything and quit stops that mixer and everything above.");
    }
    if *closing == Closing::AskLocal {
        format!(
            "GodwinMix is still running:\n\n{what}\n\nStop everything and quit ends every stream and recording and \
             closes the mixer. Keep running in the background closes this window and leaves the icon in the \
             notification area, which says what is running and stops it."
        )
    } else {
        format!("The mixer at {server} is still running:\n\n{what}\n\nQuitting closes this window and leaves it running there.")
    }
}

/// Close, quit or ask. `stop_core` passes through to `crate::quit`.
pub fn request(app: &AppHandle, stop_core: bool) {
    if ASKING.swap(true, Ordering::SeqCst) {
        crate::ui::show(app);
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (target, local) = {
            let shell = app.state::<Shell>();
            let local = shell.local.lock().unwrap().as_ref().map(|l| l.target.base.clone());
            let target = shell.target.lock().unwrap().clone();
            (target, local)
        };
        let local = local.is_some() && target.as_ref().map(|t| t.base.clone()) == local;
        let http = app.state::<Shell>().http.clone();
        let known = match &target {
            Some(target) => Some(crate::running_net::read(&http, target).await),
            None => None,
        };
        match decide(known.as_ref(), local, stop_core) {
            Closing::Quit => {
                ASKING.store(false, Ordering::SeqCst);
                crate::quit(&app, stop_core)
            }
            closing => ask(&app, known.unwrap_or(Known::Things(vec![])), closing, stop_core, target.map(|t| t.base).unwrap_or_default()),
        }
    });
}

fn ask(app: &AppHandle, known: Known, closing: Closing, stop_core: bool, server: String) {
    crate::ui::show(app);
    let buttons = match closing {
        Closing::AskLocal => MessageDialogButtons::YesNoCancelCustom(STOP_AND_QUIT.into(), BACKGROUND.into(), CANCEL.into()),
        Closing::AskStop => MessageDialogButtons::OkCancelCustom(STOP_AND_QUIT.into(), CANCEL.into()),
        _ => MessageDialogButtons::OkCancelCustom(LEAVE.into(), CANCEL.into()),
    };
    let local = closing != Closing::AskRemote;
    let handle = app.clone();
    app.dialog()
        .message(question(&known, &closing, &server))
        .title("GodwinMix is still running")
        .kind(MessageDialogKind::Warning)
        .buttons(buttons)
        .show_with_result(move |answer| {
            ASKING.store(false, Ordering::SeqCst);
            match choice(&answer, local) {
                Choice::StopAndQuit | Choice::Leave => crate::quit(&handle, stop_core),
                Choice::Background => crate::background::enter(&handle),
                Choice::Cancel => {}
            }
        });
}

#[cfg(test)]
mod tests;
