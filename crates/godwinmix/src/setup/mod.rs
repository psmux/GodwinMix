//! Setting up, on first use, the pieces that live outside this program.
//!
//! `cargo run --release -- --config godwinmix.example.toml` has to give a
//! person everything the page offers. Two kinds of piece are not in the
//! mixer's own binary:
//!
//! * the browser renderer, which draws web pages. From a checkout it is built
//!   here: the CEF download (resumable, retried), `cargo build` in `browser/`,
//!   and on macOS the app bundle. See `web/`.
//! * the first party plugins: camera, screen, audio devices, ingest and the
//!   rest. Installed from the copy shipped beside the binary or the checkout,
//!   or switched back on. See `plugin.rs`.
//!
//! One piece is set up once at a time however many callers ask; the second
//! waits on the first (`registry.rs`). Every step runs on a tokio task or a
//! child process, never on the mixer thread, a streaming thread or the bus
//! handler, and the build's own output goes to a log file under the mixer's
//! home rather than to anybody's screen. Progress is `event/setup.changed`.
//!
//! ```text
//!   registry.rs  one job a piece, the watch channel, the event
//!   plugin.rs    install or switch on a first party plugin
//!   resume.rs    sources that could not start, started again once ready
//!   run.rs       a child process with its output in the log
//!   web/         the browser renderer: look, download, unpack, build, bundle
//! ```

mod plugin;
pub mod registry;
mod resume;
mod run;
pub mod web;

pub use registry::{attach, list, start, status, wait};
pub use resume::waiting_record;

/// What went wrong, for a person and for a developer.
/// Boxed, because it rides in the `Err` of every step and an action is large.
#[derive(Debug, Clone)]
pub struct Failure(Box<Failed>);

#[derive(Debug, Clone)]
pub struct Failed {
    pub message: String,
    pub action: Option<godwinmix_protocol::ErrorAction>,
    pub detail: serde_json::Value,
}

impl Failure {
    pub fn new(message: impl Into<String>, detail: serde_json::Value) -> Self {
        Self(Box::new(Failed { message: message.into(), action: None, detail }))
    }

    pub fn with_action(mut self, action: godwinmix_protocol::ErrorAction) -> Self {
        self.0.action = Some(action);
        self
    }

    pub fn into_inner(self) -> Failed {
        *self.0
    }
}

/// The folder a piece's log goes in: `<home>/logs`.
pub fn log_path(piece: &str) -> std::path::PathBuf {
    godwinmix_host::home::dir().join("logs").join(format!("setup-{piece}.log"))
}
