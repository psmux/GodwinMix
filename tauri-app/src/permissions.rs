//! Asking macOS for the camera, the microphone and the screen, and taking the
//! person to the switch when macOS will not ask again.
//!
//! The permission belongs to this app: the plugin processes that capture are
//! below it in the process tree. macOS shows its own prompt for a camera or a
//! microphone the first time a plugin opens one, but never again after a
//! refusal, and for the screen it shows nothing a person can act on: the
//! source sits on "connecting" and nothing says why. So every few seconds,
//! while a camera, microphone or screen source is in the show and not live,
//! this asks macOS where that permission stands. Refused, or the screen not
//! yet allowed, it asks once in plain words with a button that opens the
//! exact page of System Settings. Allowed while the source is still not live,
//! it offers to restart, since macOS applies a screen recording permission to
//! an app only when it starts again.
//!
//! The same on Intel and Apple silicon. Off macOS, `check` does nothing.

use tauri::AppHandle;

/// What a source needs from macOS.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Need {
    Camera,
    Microphone,
    Screen,
}

impl Need {
    /// The need behind a source's kind, from `/api/status`.
    pub fn of(kind: &str) -> Option<Need> {
        match kind.split('/').next()? {
            "camera" => Some(Need::Camera),
            "audio-device" => Some(Need::Microphone),
            "screen" => Some(Need::Screen),
            _ => None,
        }
    }

    /// The page of System Settings with this permission's switch on it.
    pub fn settings_url(self) -> &'static str {
        match self {
            Need::Camera => "x-apple.systempreferences:com.apple.preference.security?Privacy_Camera",
            Need::Microphone => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            Need::Screen => "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
        }
    }

    /// What to ask, in the words a person reads before a service.
    pub fn ask(self) -> (&'static str, &'static str) {
        match self {
            Need::Camera => (
                "Let GodwinMix use the camera",
                "A camera is in your show, and macOS is not letting GodwinMix use it.\n\n\
                 Click Open System Settings, then switch on GodwinMix under Camera. \
                 The camera comes up by itself once it is on.",
            ),
            Need::Microphone => (
                "Let GodwinMix use the microphone",
                "A microphone or sound input is in your show, and macOS is not letting \
                 GodwinMix hear it.\n\nClick Open System Settings, then switch on GodwinMix \
                 under Microphone. The sound comes through by itself once it is on.",
            ),
            Need::Screen => (
                "Let GodwinMix record the screen",
                "A screen capture is in your show, and macOS has not given GodwinMix \
                 permission to record the screen yet.\n\nClick Open System Settings, then \
                 switch on GodwinMix under Screen & System Audio Recording. If macOS asks \
                 to quit and reopen GodwinMix, say yes; GodwinMix will also offer to \
                 restart once the switch is on.",
            ),
        }
    }
}

/// Where macOS stands on one permission.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Standing {
    /// Never asked. macOS asks by itself when a plugin opens the device.
    Unasked,
    Allowed,
    /// Refused, or barred by a profile: macOS will not ask again.
    Refused,
}

/// What to do about one need this tick, given what was done before.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Nothing,
    /// Ask macOS (the screen only), then explain with the button.
    Ask,
    /// Allowed, and the source still not live: offer a restart.
    OfferRestart,
}

/// What this app has already done about a need, so nobody is asked twice.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Done {
    pub asked: bool,
    pub restart_offered: bool,
    /// Ticks seen with the permission allowed and the source still not live.
    pub allowed_waiting: u32,
}

/// Ticks to wait, allowed but not live, before offering a restart. A camera
/// takes a second or two to open; five seconds a tick, this is ten seconds.
const PATIENCE: u32 = 2;

/// The decision, kept apart from macOS so it can be tested anywhere.
pub fn step(need: Need, standing: Standing, live: bool, done: &mut Done) -> Step {
    if live {
        done.allowed_waiting = 0;
        return Step::Nothing;
    }
    match standing {
        Standing::Refused if !done.asked => {
            done.asked = true;
            Step::Ask
        }
        // A camera or a microphone never asked gets macOS's own prompt when
        // its plugin opens it; the screen gets nothing, so it is asked here.
        Standing::Unasked if need == Need::Screen && !done.asked => {
            done.asked = true;
            Step::Ask
        }
        Standing::Allowed if done.asked && !done.restart_offered => {
            done.allowed_waiting += 1;
            if done.allowed_waiting > PATIENCE {
                done.restart_offered = true;
                Step::OfferRestart
            } else {
                Step::Nothing
            }
        }
        _ => Step::Nothing,
    }
}

/// One look, from the background watcher. Does nothing off macOS.
pub fn check(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    mac::check(app.clone());
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{step, Done, Need, Standing, Step};
    use crate::Shell;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tauri::{AppHandle, Manager};
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};
    use tauri_plugin_opener::OpenerExt;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    // AVCaptureDevice lives here; linking it is what makes the class exist.
    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}

    static DONE: Mutex<Option<HashMap<Need, Done>>> = Mutex::new(None);

    /// AVFoundation's answer for "vide" (AVMediaTypeVideo) or "soun"
    /// (AVMediaTypeAudio): 0 not determined, 1 restricted, 2 denied,
    /// 3 authorized.
    fn av_standing(media: &str) -> Standing {
        use objc2::{class, msg_send};
        let ty = objc2_foundation::NSString::from_str(media);
        let status: isize = unsafe { msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: &*ty] };
        match status {
            3 => Standing::Allowed,
            0 => Standing::Unasked,
            _ => Standing::Refused,
        }
    }

    fn standing(need: Need) -> Standing {
        match need {
            Need::Camera => av_standing("vide"),
            Need::Microphone => av_standing("soun"),
            // macOS answers allowed or not for the screen; whether it was ever
            // asked is not something it says, so not allowed counts as never
            // asked and is asked once.
            Need::Screen => {
                if unsafe { CGPreflightScreenCaptureAccess() } {
                    Standing::Allowed
                } else {
                    Standing::Unasked
                }
            }
        }
    }

    pub fn check(app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            let Some(sources) = sources(&app).await else { return };
            for (need, live) in sources {
                let decided = {
                    let mut all = DONE.lock().unwrap();
                    let done = all.get_or_insert_with(HashMap::new).entry(need).or_default();
                    step(need, standing(need), live, done)
                };
                match decided {
                    Step::Ask => ask(&app, need),
                    Step::OfferRestart => offer_restart(&app, need).await,
                    Step::Nothing => {}
                }
            }
        });
    }

    /// Each need in the show, and whether any source with it is live.
    async fn sources(app: &AppHandle) -> Option<Vec<(Need, bool)>> {
        let target = app.state::<Shell>().target.lock().unwrap().clone()?;
        let http = app.state::<Shell>().http.clone();
        let mut req = http.get(format!("{}/api/status", target.base)).timeout(std::time::Duration::from_secs(3));
        if !target.token.is_empty() {
            req = req.bearer_auth(&target.token);
        }
        let status: serde_json::Value = req.send().await.ok()?.json().await.ok()?;
        let mut out: HashMap<Need, bool> = HashMap::new();
        for s in status.get("sources")?.as_array()? {
            let kind = s.get("type").and_then(|v| v.as_str()).unwrap_or_default();
            if let Some(need) = Need::of(kind) {
                let live = s.get("state").and_then(|v| v.as_str()) == Some("live");
                *out.entry(need).or_default() |= live;
            }
        }
        Some(out.into_iter().collect())
    }

    #[cfg(test)]
    #[test]
    fn macos_answers_where_each_permission_stands_without_prompting() {
        // Status only: none of these puts up a prompt. A wrong class or
        // selector would panic here rather than in front of an operator.
        for need in [Need::Camera, Need::Microphone, Need::Screen] {
            let _ = standing(need);
        }
    }

    fn ask(app: &AppHandle, need: Need) {
        if need == Need::Screen {
            // The first time, macOS puts up its own message with a button to
            // the same place. After that it does nothing, which is why the
            // dialog below always follows.
            unsafe { CGRequestScreenCaptureAccess() };
        }
        let (title, body) = need.ask();
        let handle = app.clone();
        app.dialog()
            .message(body)
            .title(title)
            .kind(MessageDialogKind::Info)
            .buttons(MessageDialogButtons::OkCancelCustom("Open System Settings".into(), "Not now".into()))
            .show_with_result(move |answer| {
                let open = matches!(&answer, MessageDialogResult::Ok)
                    || matches!(&answer, MessageDialogResult::Custom(s) if s == "Open System Settings");
                if open {
                    if let Err(e) = handle.opener().open_url(need.settings_url(), None::<&str>) {
                        eprintln!("[desktop] could not open System Settings: {e}");
                    }
                }
            });
    }

    async fn offer_restart(app: &AppHandle, need: Need) {
        let what = match need {
            Need::Camera => "the camera",
            Need::Microphone => "the microphone",
            Need::Screen => "the screen",
        };
        let streaming = crate::background::is_streaming(app).await;
        let mut body = format!(
            "GodwinMix is now allowed to use {what}, and macOS applies that once \
             GodwinMix starts again. Restart now?"
        );
        if streaming {
            body.push_str("\n\nYou are streaming. Restarting stops it for a few seconds, and it starts again with the mixer.");
        }
        let handle = app.clone();
        app.dialog()
            .message(body)
            .title("Restart GodwinMix")
            .kind(MessageDialogKind::Info)
            .buttons(MessageDialogButtons::OkCancelCustom("Restart GodwinMix".into(), "Later".into()))
            .show_with_result(move |answer| {
                let yes = matches!(&answer, MessageDialogResult::Ok)
                    || matches!(&answer, MessageDialogResult::Custom(s) if s == "Restart GodwinMix");
                if yes {
                    crate::relaunch(&handle);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_kind_names_what_it_needs() {
        assert_eq!(Need::of("camera/source"), Some(Need::Camera));
        assert_eq!(Need::of("audio-device/source"), Some(Need::Microphone));
        assert_eq!(Need::of("screen/source"), Some(Need::Screen));
        assert_eq!(Need::of("browser/source"), None);
        assert!(Need::Screen.settings_url().ends_with("Privacy_ScreenCapture"));
    }

    #[test]
    fn a_refusal_is_explained_once_and_never_again() {
        let mut done = Done::default();
        assert_eq!(step(Need::Camera, Standing::Refused, false, &mut done), Step::Ask);
        assert_eq!(step(Need::Camera, Standing::Refused, false, &mut done), Step::Nothing);
    }

    #[test]
    fn a_camera_never_asked_is_left_to_macos_and_the_screen_is_asked() {
        let mut camera = Done::default();
        assert_eq!(step(Need::Camera, Standing::Unasked, false, &mut camera), Step::Nothing);
        let mut screen = Done::default();
        assert_eq!(step(Need::Screen, Standing::Unasked, false, &mut screen), Step::Ask);
    }

    #[test]
    fn allowed_but_still_dark_is_offered_one_restart_after_a_wait() {
        let mut done = Done::default();
        assert_eq!(step(Need::Screen, Standing::Unasked, false, &mut done), Step::Ask);
        let mut seen = Vec::new();
        for _ in 0..6 {
            seen.push(step(Need::Screen, Standing::Allowed, false, &mut done));
        }
        assert_eq!(seen.iter().filter(|s| **s == Step::OfferRestart).count(), 1, "{seen:?}");
        assert_eq!(seen[0], Step::Nothing, "a camera gets a moment to open first");
    }

    #[test]
    fn a_live_source_needs_nothing() {
        let mut done = Done::default();
        assert_eq!(step(Need::Screen, Standing::Unasked, true, &mut done), Step::Nothing);
        assert!(!done.asked);
    }
}
