//! The browser renderer behind one HTML graphic: the process, the pipe its
//! pictures come down, and the line its state goes up.
//!
//! The state is written by a thread of its own, so `configure` and the
//! mixer's cue hand it a line and return at once whatever the process is
//! doing. Only the newest line matters (each is the whole state), so a
//! renderer that is slow to read is sent the latest, not a queue.

use super::frames::{self, Feed};
use crate::caps::CanvasCaps;
use crate::config::BrowserConfig;
use crate::input::{spawn_exec, ExecChild, ExecSpec, ExecStdout};
use crate::overlay::carrier::Carrier;
use crate::overlay::Layer;
use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;

/// The width a template is designed on. A canvas of another width draws the
/// page scaled, laid out exactly as designed.
pub const DESIGN_WIDTH: u32 = 1920;

/// What the renderer is asked to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub url: String,
    /// Frames a second at most; zero for the canvas rate.
    pub fps: u32,
    /// A template: a local file, drawn with no network at all.
    pub offline: bool,
    /// Laid out on a 1920 wide page and scaled to the canvas.
    pub designed: bool,
    /// Covers the whole picture: sent whole as I420, to the compositor.
    pub opaque: bool,
}

impl Page {
    /// An HTML template's file.
    pub fn template(file: &Path, fps: u32, opaque: bool) -> Page {
        Page { url: crate::input::file_uri(file), fps, offline: true, designed: true, opaque }
    }
}

pub struct Renderer {
    /// Dropping it kills the process.
    _child: ExecChild,
    lines: Sender<String>,
    pub feed: Arc<Feed>,
}

impl Renderer {
    /// Start the renderer on `page` at the canvas size, its pictures going to
    /// `layer`, and tell it `state` straight away.
    pub fn start(id: &str, page: &Page, canvas: &CanvasCaps, browser: &BrowserConfig, layer: Arc<Layer>, carrier: Arc<Carrier>, state: String) -> Result<Renderer> {
        let spec = spec(page, canvas, browser)?;
        let (stdout, mut child, stderr) = spawn_exec(id, &spec)?;
        let stdin = child.stdin.take().context("the renderer has no stdin to send the graphic's state on")?;
        let feed = Feed::new();
        frames::start(id, reader(stdout), layer, carrier, feed.clone());
        let lines = writer(id, stdin);
        let _ = lines.send(state);
        Ok(Renderer { _child: ExecChild::new(child, spec.env.clone(), None, stderr), lines, feed })
    }

    /// Tell the page its whole state. Never waits.
    pub fn send(&self, state: String) {
        let _ = self.lines.send(state);
    }

    /// Whether the renderer has gone away by itself.
    pub fn ended(&self) -> bool {
        self.feed.ended.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.feed.end();
    }
}

/// The command line: the page, the canvas size, graphic mode, and for a
/// template, no network and the scale that lays a 1920 wide design out on
/// this canvas.
fn spec(page: &Page, canvas: &CanvasCaps, browser: &BrowserConfig) -> Result<ExecSpec> {
    let program = crate::input::graphic_renderer(browser)?;
    let fps = if page.fps == 0 { canvas.fps.numer().max(1) as u32 } else { page.fps };
    let mut argv = vec![
        program.to_string_lossy().to_string(),
        "--url".into(),
        page.url.clone(),
        "--width".into(),
        canvas.width.to_string(),
        "--height".into(),
        canvas.height.to_string(),
        "--fps".into(),
        fps.to_string(),
        "--graphic".into(),
    ];
    if page.offline {
        argv.push("--offline".into());
    }
    if page.opaque {
        argv.push("--opaque".into());
    }
    if page.designed && canvas.width as u32 != DESIGN_WIDTH {
        argv.extend(["--scale".into(), format!("{:.4}", canvas.width as f64 / DESIGN_WIDTH as f64)]);
    }
    argv.extend(browser.args.iter().cloned());
    Ok(ExecSpec { argv, env: browser.env.clone(), pipe_stdin: true, cwd: None })
}

fn reader(out: ExecStdout) -> Box<dyn Read + Send> {
    match out {
        #[cfg(unix)]
        ExecStdout::Fd(fd) => Box::new(std::fs::File::from(fd)),
        #[cfg(not(unix))]
        ExecStdout::Pipe(p) => Box::new(p),
    }
}

/// A thread writing the newest state line to the renderer's stdin.
fn writer(id: &str, mut stdin: std::process::ChildStdin) -> Sender<String> {
    let (tx, rx) = channel::<String>();
    let spawned = std::thread::Builder::new().name(format!("gmx-html-state-{id}")).spawn(move || {
        while let Ok(mut line) = rx.recv() {
            while let Ok(newer) = rx.try_recv() {
                line = newer;
            }
            if writeln!(stdin, "{line}").and_then(|_| stdin.flush()).is_err() {
                return;
            }
        }
    });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "could not start the HTML graphic's state writer");
    }
    tx
}
