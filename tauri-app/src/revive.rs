//! Starting the mixer again after it died on its own.
//!
//! The daemon can end in ways nobody asked for: tokio aborts the whole
//! process when its I/O driver fails (on Windows a kernel that is short of
//! memory answers a socket poll with error 1450), GStreamer can crash in a
//! plugin, a person can end it in Task Manager. Every one of those took the
//! programme off air and left it off, because this app only started the
//! mixer again for the exit codes that ask for it. Now an exit this app did
//! not cause is answered with a restart on the same port and token, so the
//! page reconnects by itself and the programme is back in seconds.
//!
//! Left alone: a clean exit (`core.shutdown`, status zero), an exit while
//! this app is stopping or restarting the mixer itself (it has taken the
//! mixer out of `Shell::local` by then), and a second crash within
//! [`TOO_SOON`] of the last restart, which says the mixer falls over as soon
//! as it is up and starting it again would only loop.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::MessageDialogKind;

use crate::Shell;

/// A crash this soon after the last restart is not answered with another.
pub const TOO_SOON: Duration = Duration::from_secs(60);

/// When the last restart after a crash was made.
static LAST: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Debug, PartialEq, Eq)]
pub enum Revive {
    /// Not a crash, or not one this app should answer.
    Leave,
    /// Start it again.
    Restart,
    /// It crashed again straight after the last restart.
    GiveUp,
}

/// What to do about an exit. `ours` is whether the process that ended is
/// still the mixer this app holds; `since_last` is how long ago the last
/// restart after a crash was, if there was one.
pub fn decide(code: Option<i32>, ours: bool, since_last: Option<Duration>) -> Revive {
    if code == Some(0) || !ours {
        return Revive::Leave;
    }
    match since_last {
        Some(ago) if ago < TOO_SOON => Revive::GiveUp,
        _ => Revive::Restart,
    }
}

/// The mixer with process id `pid` exited with `code`, and it was not one of
/// the codes that ask for a restart.
pub fn after_exit(app: &AppHandle, pid: u32, code: Option<i32>) {
    let ours = app.state::<Shell>().local.lock().unwrap().as_ref().and_then(|l| l.pid()) == Some(pid);
    let mut last = LAST.lock().unwrap();
    match decide(code, ours, last.map(|t| t.elapsed())) {
        Revive::Leave => {}
        Revive::Restart => {
            *last = Some(Instant::now());
            eprintln!("[desktop] the mixer ended on its own ({code:?}); starting it again");
            crate::restart::after_exit(app.clone());
        }
        Revive::GiveUp => {
            let why = format!(
                "The mixer stopped again less than {} seconds after it was started again, so it was \
                 left stopped. What it said is in the mixer log. Restart it from the menu once the \
                 cause is fixed.",
                TOO_SOON.as_secs()
            );
            crate::ui::tell(app, "The mixer keeps stopping", &why, MessageDialogKind::Error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_of_the_mixer_this_app_holds_is_answered_with_a_restart() {
        // An abort on Windows, a signal on Unix, a kill from Task Manager.
        assert_eq!(decide(Some(-1073740791), true, None), Revive::Restart);
        assert_eq!(decide(None, true, None), Revive::Restart);
        assert_eq!(decide(Some(1), true, Some(Duration::from_secs(600))), Revive::Restart);
    }

    #[test]
    fn a_clean_exit_or_a_mixer_this_app_was_already_stopping_is_left_alone() {
        assert_eq!(decide(Some(0), true, None), Revive::Leave);
        assert_eq!(decide(Some(1), false, None), Revive::Leave);
    }

    #[test]
    fn a_crash_straight_after_the_last_restart_is_not_answered_with_another() {
        assert_eq!(decide(Some(1), true, Some(Duration::from_secs(5))), Revive::GiveUp);
    }
}
