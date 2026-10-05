//! Graphic mode's control channel: the mixer writes the graphic's state on
//! stdin, one JSON object a line, and the page is told.
//!
//! ```text
//!   {"fields": {"name": "Ada Lovelace"}, "cue": "in"}
//! ```
//!
//! Each line is the whole state, not a change, so the newest line is all that
//! has to be kept: a page that loads (or loads again) is given it straight
//! away, and a line that arrives before the page has loaded waits for it.
//! The line is not parsed here. The mixer wrote it with a JSON serialiser and
//! JSON is a JavaScript expression, so it is handed to `__gmxApply` as is.

use cef::*;
use std::cell::RefCell;
use std::io::BufRead;
use std::sync::Mutex;

/// Applies the state; see `graphic-runtime.js`.
const RUNTIME_JS: &str = include_str!("graphic-runtime.js");

/// The newest state line from the mixer.
static STATE: Mutex<Option<String>> = Mutex::new(None);

thread_local! {
    /// The browser, kept on CEF's UI thread, which is the only thread that
    /// creates it and the only one the tasks below run on.
    static BROWSER: RefCell<Option<Browser>> = const { RefCell::new(None) };
}

/// Keep the browser for the tasks. Called on the UI thread.
pub fn remember(browser: &Browser) {
    BROWSER.with(|b| *b.borrow_mut() = Some(browser.clone()));
}

/// Read stdin until it closes, passing each state on to the page.
pub fn listen() {
    std::thread::Builder::new()
        .name("graphic-control".into())
        .spawn(|| {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                let line = line.trim();
                if !(line.starts_with('{') && line.ends_with('}')) {
                    if !line.is_empty() {
                        eprintln!("[browser] ignored a control line that is not a JSON object");
                    }
                    continue;
                }
                *STATE.lock().unwrap() = Some(line.to_string());
                let mut task = Apply::new();
                post_task(ThreadId::UI, Some(&mut task));
            }
        })
        .expect("spawning the control reader");
}

/// Chromium switches for graphic mode: nothing in the background reaches
/// out. With `offline`, for a template that is a local file, every host name
/// fails to resolve, so a page cannot fetch a font or a script at show time
/// (and works the same in a building with no internet), and a page may read
/// the files beside it, which lets it load its own scripts as modules and its
/// pictures into WebGL.
pub fn switches(cl: &mut CommandLine, offline: bool) {
    for sw in ["disable-background-networking", "disable-component-update", "disable-extensions", "disable-sync"] {
        cl.append_switch(Some(&CefString::from(sw)));
    }
    if offline {
        cl.append_switch(Some(&CefString::from("allow-file-access-from-files")));
        cl.append_switch_with_value(Some(&CefString::from("host-resolver-rules")), Some(&CefString::from("MAP * ~NOTFOUND")));
    }
}

/// The script that installs the runtime and applies the newest state.
fn script() -> String {
    let state = STATE.lock().unwrap().clone().unwrap_or_else(|| "{}".into());
    format!("{RUNTIME_JS}\nwindow.__gmxApply({state});")
}

/// Whether the page in the main frame has finished loading. A state that
/// arrives before then waits for `on_load`, so the page's own scripts have
/// added their listeners before `gmx:in` fires.
static LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The main frame started loading a page.
pub fn on_load_start() {
    LOADED.store(false, std::sync::atomic::Ordering::SeqCst);
}

/// A page finished loading: give it the runtime and the state.
pub fn on_load(frame: &Frame) {
    LOADED.store(true, std::sync::atomic::Ordering::SeqCst);
    apply(frame);
}

fn apply(frame: &Frame) {
    frame.execute_java_script(Some(&script().as_str().into()), Some(&"gmx://graphic-runtime.js".into()), 0);
}

wrap_task! {
    struct Apply;

    impl Task {
        fn execute(&self) {
            if !LOADED.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            BROWSER.with(|b| {
                let Some(browser) = b.borrow().clone() else { return };
                if let Some(frame) = browser.main_frame() {
                    apply(&frame);
                }
            });
        }
    }
}
