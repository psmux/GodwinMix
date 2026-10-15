//! A web page source's control line: the operator's clicks, keys and
//! navigation going down to the renderer, and where the page is coming back.
//!
//! Down: the renderer runs with `--control` and a stdin of its own, and reads
//! one JSON line per event (`godwinmix_protocol::page`). Nothing is started
//! for it until the first event: the pipe is held, and the thread that writes
//! to it is made the first time somebody touches the page. A source nobody
//! interacts with costs one idle pipe.
//!
//! The thread is the only thing that ever waits on the renderer. The mixer
//! hands lines to a bounded queue and returns at once; a renderer too busy to
//! read fills the queue and further events are dropped and counted, never
//! waited for. The programme cannot be held up by a click.
//!
//! Up: the renderer writes `[browser] page {json}` on stderr whenever the
//! address or the loading state changes. The stderr thread hands those lines
//! to `watch`, which keeps the newest.

use crate::input::LineHook;
use godwinmix_protocol::page::PageState;
use parking_lot::Mutex;
use std::io::Write;
use std::process::ChildStdin;
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::Arc;
use tracing::debug;

/// Lines that may wait for the renderer before more are dropped. A second or
/// two of pointer moves at the rate a browser sends them.
const QUEUE: usize = 256;

/// The prefix on the renderer's report of where the page is.
const PAGE_LINE: &str = "[browser] page ";

#[derive(Default)]
pub struct PageControl {
    state: Arc<Mutex<PageState>>,
    /// The renderer's stdin, until the first event takes it to a writer.
    stdin: Option<ChildStdin>,
    lines: Option<SyncSender<String>>,
}

impl PageControl {
    /// Hand every stderr line here, so the state follows the page.
    pub fn watch(&self) -> LineHook {
        let state = self.state.clone();
        Arc::new(move |line: &str| {
            if let Some(page) = report(line) {
                *state.lock() = page;
            }
        })
    }

    /// A renderer was started: this is its stdin. Any writer to the last one
    /// is let go, and ends when its pipe does.
    pub fn attach(&mut self, stdin: Option<ChildStdin>) {
        self.lines = None;
        self.stdin = stdin;
    }

    /// Where the page is.
    pub fn state(&self) -> PageState {
        self.state.lock().clone()
    }

    /// Take the page to `url` as far as anybody asking is concerned, before
    /// the renderer has said it got there, so a save straight after keeps it.
    pub fn going_to(&self, url: &str) {
        self.state.lock().url = url.to_string();
    }

    /// Queue lines for the renderer. Answers how many were queued and how
    /// many were dropped because it is not keeping up.
    pub fn send(&mut self, id: &str, lines: Vec<String>) -> anyhow::Result<(usize, usize)> {
        let tx = match &self.lines {
            Some(tx) => tx.clone(),
            None => self.start(id)?,
        };
        let (mut sent, mut dropped) = (0, 0);
        for line in lines {
            match tx.try_send(line) {
                Ok(()) => sent += 1,
                Err(TrySendError::Full(_)) => dropped += 1,
                Err(TrySendError::Disconnected(_)) => {
                    self.lines = None;
                    anyhow::bail!(
                        "the page's renderer has stopped reading. It is being started again; \
                         try again when the source is live"
                    );
                }
            }
        }
        Ok((sent, dropped))
    }

    /// The writer, made on first use.
    fn start(&mut self, id: &str) -> anyhow::Result<SyncSender<String>> {
        let stdin = self.stdin.take().ok_or_else(|| {
            anyhow::anyhow!(
                "this page's renderer was started without a control line, so it cannot be \
                 clicked or typed into. Restart the source to give it one"
            )
        })?;
        let (tx, rx) = sync_channel::<String>(QUEUE);
        let name = format!("gmx-page-input-{id}");
        let who = id.to_string();
        std::thread::Builder::new().name(name).spawn(move || {
            let mut stdin = stdin;
            while let Ok(line) = rx.recv() {
                if writeln!(stdin, "{line}").and_then(|_| stdin.flush()).is_err() {
                    debug!(source = %who, "the page's renderer closed its control line");
                    break;
                }
            }
        })?;
        self.lines = Some(tx.clone());
        Ok(tx)
    }
}

/// The page state on one of the renderer's stderr lines, if it is one.
pub fn report(line: &str) -> Option<PageState> {
    let (_, json) = line.split_once(PAGE_LINE)?;
    serde_json::from_str(json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_renderers_report_is_read_and_anything_else_is_not() {
        let page = report(r#"[browser] page {"url":"https://example.com/","can_back":true,"can_forward":false,"loading":false}"#).unwrap();
        assert_eq!(page.url, "https://example.com/");
        assert!(page.can_back && !page.loading);
        assert!(report("[browser] load finished, http 200").is_none());
        assert!(report("[browser] page not json").is_none());
    }

    #[test]
    fn the_watch_keeps_the_newest_report() {
        let control = PageControl::default();
        let hook = control.watch();
        hook(r#"[browser] page {"url":"https://a.example/"}"#);
        hook("some other noise");
        hook(r#"[browser] page {"url":"https://b.example/","loading":true}"#);
        assert_eq!(control.state().url, "https://b.example/");
        assert!(control.state().loading);
    }

    #[test]
    fn a_renderer_with_no_stdin_says_how_to_get_one() {
        let mut control = PageControl::default();
        let e = control.send("site", vec!["{}".into()]).unwrap_err().to_string();
        assert!(e.contains("Restart the source"), "{e}");
    }

    /// The writer end to end, against a real child that echoes what it reads.
    #[test]
    fn lines_reach_the_childs_stdin_in_order() {
        let mut child = if cfg!(windows) {
            std::process::Command::new("findstr").arg("^").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn()
        } else {
            std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn()
        }
        .expect("a child that echoes its stdin");
        let mut control = PageControl::default();
        control.attach(child.stdin.take());
        let lines: Vec<String> = (0..10).map(|i| format!(r#"{{"type":"move","x":0.{i},"y":0.5}}"#)).collect();
        assert_eq!(control.send("site", lines.clone()).unwrap(), (10, 0));
        // Letting go of the writer closes the child's stdin, and it ends.
        control.attach(None);
        let out = child.wait_with_output().unwrap();
        let got: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim().to_string()).collect();
        assert_eq!(got, lines);
    }
}
