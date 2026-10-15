//! The control lines of `page.rs`, handed to Chromium.
//!
//! A thread reads stdin and queues commands; CEF's UI thread drains the queue,
//! which is the only thread its input calls may run on. The reader never waits
//! on the browser and the browser never waits on the reader, so nothing here
//! can hold up a frame. A run of pointer moves that arrives faster than the UI
//! thread drains is kept as the last one, which is the only one that matters.
//!
//! Where the page is, which address and whether it can go back, is written on
//! stderr as `[browser] page {json}` whenever it changes, for the mixer to
//! show beside the address bar and keep with the source.

use crate::page::{self, Button, Cmd, Mods};
use cef::*;
use std::collections::VecDeque;
use std::io::BufRead;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::Mutex;

static QUEUE: Mutex<VecDeque<Cmd>> = Mutex::new(VecDeque::new());
/// A drain is posted and has not run yet.
static POSTED: AtomicBool = AtomicBool::new(false);
/// The page's layout size, which a share of the page is a share of.
static VIEW_W: AtomicI32 = AtomicI32::new(1280);
static VIEW_H: AtomicI32 = AtomicI32::new(720);
/// Buttons held down, as CEF's event flags, so a move while one is held is a
/// drag (a seek bar, a volume slider, a text selection).
static HELD: AtomicU32 = AtomicU32::new(0);
/// The last point, for a leave.
static LAST: Mutex<(i32, i32)> = Mutex::new((0, 0));

/// Read control lines from stdin until it closes. `view` is the layout size.
pub fn listen(view: (i32, i32)) {
    VIEW_W.store(view.0.max(1), Ordering::Relaxed);
    VIEW_H.store(view.1.max(1), Ordering::Relaxed);
    let spawned = std::thread::Builder::new().name("page-control".into()).spawn(|| {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match page::parse(&line) {
                Ok(cmd) => queue(cmd),
                Err(e) => eprintln!("[browser] ignored a control line: {e}"),
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("[browser] no control channel: {e}");
    }
}

fn queue(cmd: Cmd) {
    {
        let mut q = QUEUE.lock().unwrap();
        let both_moves = matches!(cmd, Cmd::Move { .. }) && matches!(q.back(), Some(Cmd::Move { .. }));
        if both_moves {
            q.pop_back();
        }
        q.push_back(cmd);
    }
    if !POSTED.swap(true, Ordering::AcqRel) {
        let mut task = Drain::new();
        post_task(ThreadId::UI, Some(&mut task));
    }
}

wrap_task! {
    struct Drain;

    impl Task {
        fn execute(&self) {
            POSTED.store(false, Ordering::Release);
            let cmds: Vec<Cmd> = QUEUE.lock().unwrap().drain(..).collect();
            crate::control::with_browser(|browser| {
                for cmd in cmds {
                    run(browser, cmd);
                }
            });
        }
    }
}

fn flags(m: Mods) -> u32 {
    use sys::cef_event_flags_t as F;
    let mut f = HELD.load(Ordering::Relaxed);
    for (on, flag) in [(m.shift, F::EVENTFLAG_SHIFT_DOWN), (m.ctrl, F::EVENTFLAG_CONTROL_DOWN), (m.alt, F::EVENTFLAG_ALT_DOWN), (m.meta, F::EVENTFLAG_COMMAND_DOWN)] {
        if on {
            f |= flag.0 as u32;
        }
    }
    f
}

fn button(b: Button) -> (MouseButtonType, u32) {
    use sys::cef_event_flags_t as F;
    use sys::cef_mouse_button_type_t as T;
    match b {
        Button::Left => (MouseButtonType::from(T::MBT_LEFT), F::EVENTFLAG_LEFT_MOUSE_BUTTON.0 as u32),
        Button::Middle => (MouseButtonType::from(T::MBT_MIDDLE), F::EVENTFLAG_MIDDLE_MOUSE_BUTTON.0 as u32),
        Button::Right => (MouseButtonType::from(T::MBT_RIGHT), F::EVENTFLAG_RIGHT_MOUSE_BUTTON.0 as u32),
    }
}

fn mouse(x: f64, y: f64, mods: Mods) -> MouseEvent {
    let (x, y) = page::to_view(x, y, (VIEW_W.load(Ordering::Relaxed), VIEW_H.load(Ordering::Relaxed)));
    *LAST.lock().unwrap() = (x, y);
    MouseEvent { x, y, modifiers: flags(mods) }
}

fn run(browser: &Browser, cmd: Cmd) {
    let Some(host) = browser.host() else { return };
    match cmd {
        Cmd::Move { x, y, mods } => host.send_mouse_move_event(Some(&mouse(x, y, mods)), 0),
        Cmd::Down { x, y, button: b, count, mods } => {
            host.set_focus(1);
            let ev = mouse(x, y, mods);
            host.send_mouse_move_event(Some(&ev), 0);
            let (kind, flag) = button(b);
            HELD.fetch_or(flag, Ordering::Relaxed);
            host.send_mouse_click_event(Some(&ev), kind, 0, count.max(1) as i32);
        }
        Cmd::Up { x, y, button: b, count, mods } => {
            let (kind, flag) = button(b);
            HELD.fetch_and(!flag, Ordering::Relaxed);
            host.send_mouse_click_event(Some(&mouse(x, y, mods)), kind, 1, count.max(1) as i32);
        }
        Cmd::Wheel { x, y, dx, dy } => {
            host.send_mouse_wheel_event(Some(&mouse(x, y, Mods::default())), page::wheel_delta(dx), page::wheel_delta(dy));
        }
        Cmd::Leave => {
            let (x, y) = *LAST.lock().unwrap();
            HELD.store(0, Ordering::Relaxed);
            host.send_mouse_move_event(Some(&MouseEvent { x, y, modifiers: 0 }), 1);
        }
        Cmd::Key { code, key, mods } => {
            host.set_focus(1);
            press(&host, code, page::typed_char(code, key.as_deref(), mods), flags(mods));
        }
        Cmd::Text { text } => {
            host.set_focus(1);
            for unit in text.encode_utf16() {
                key(&host, sys::cef_key_event_type_t::KEYEVENT_CHAR, 0, unit, 0);
            }
        }
        Cmd::Go { url } => crate::web::load(browser, &url),
        Cmd::Back => browser.go_back(),
        Cmd::Forward => browser.go_forward(),
        Cmd::Reload => browser.reload(),
    }
}

/// A key down, the character it types if it types one, and the key up: the
/// three events a real key press makes.
fn press(host: &BrowserHost, code: u16, typed: Option<u16>, mods: u32) {
    use sys::cef_key_event_type_t as K;
    key(host, K::KEYEVENT_RAWKEYDOWN, code as i32, typed.unwrap_or(0), mods);
    if let Some(c) = typed {
        key(host, K::KEYEVENT_CHAR, code as i32, c, mods);
    }
    key(host, K::KEYEVENT_KEYUP, code as i32, typed.unwrap_or(0), mods);
}

fn key(host: &BrowserHost, kind: sys::cef_key_event_type_t, code: i32, c: u16, mods: u32) {
    let ev = KeyEvent {
        type_: KeyEventType::from(kind),
        modifiers: mods,
        windows_key_code: if matches!(kind, sys::cef_key_event_type_t::KEYEVENT_CHAR) { c as i32 } else { code },
        character: c,
        unmodified_character: c,
        ..Default::default()
    };
    host.send_key_event(Some(&ev));
}

/// Where the page is, for the mixer. Only written when it changed.
#[derive(Clone, Default, PartialEq)]
struct Where {
    url: String,
    back: bool,
    forward: bool,
    loading: bool,
    /// Why the page cannot play what it shows, when it has said.
    notice: String,
}

static STATE: Mutex<Option<Where>> = Mutex::new(None);

fn report(change: impl FnOnce(&mut Where)) {
    let mut guard = STATE.lock().unwrap();
    let s = guard.get_or_insert_with(Where::default);
    let before = s.clone();
    change(s);
    if *s == before {
        return;
    }
    let line = serde_json::json!({
        "url": s.url, "can_back": s.back, "can_forward": s.forward, "loading": s.loading,
        "notice": (!s.notice.is_empty()).then_some(&s.notice),
    });
    eprintln!("[browser] page {line}");
}

/// The main frame's address changed: a link, a redirect, back or forward.
/// A notice about the last page goes with it.
pub fn on_address(url: &str) {
    if url == "about:blank" {
        return;
    }
    report(|s| {
        if s.url != url {
            s.notice.clear();
        }
        s.url = url.to_string();
    });
}

/// The page started or stopped loading.
pub fn on_loading(loading: bool, back: bool, forward: bool) {
    report(|s| {
        s.back = back;
        s.forward = forward;
        s.loading = loading;
    });
}

/// The page said why it cannot play something. See `codec-watch.js`.
pub fn on_notice(text: &str) {
    report(|s| s.notice = text.chars().take(300).collect());
}
