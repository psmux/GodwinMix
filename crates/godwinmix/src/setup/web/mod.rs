//! The browser renderer, set up from the checkout the first time a web page
//! is added.
//!
//! ```text
//!   1. the web page engine (CEF): its index, then its archive, downloaded
//!      with retries and carried on from where a dropped connection left it,
//!      checked against the published digest, unpacked where its build looks
//!   2. cargo build --release in browser/, with nothing left to download
//!   3. on macOS, browser/dev/mac-bundle.sh, because CEF runs only from an app
//! ```
//!
//! Each step that is already done is skipped, so Try again after a failure
//! picks up where it stopped. A package carries the renderer and never comes
//! here. See `godwinmix_core::setup::web` for where it is looked for.

mod build;
mod cef;
mod download;

use super::registry::Progress;
use super::Failure;
use godwinmix_core::config::BrowserConfig;
use godwinmix_core::setup::{names, plain, web};
use godwinmix_protocol::setup::{SetupState, SetupStatus};
use godwinmix_protocol::ErrorAction;
use parking_lot::Mutex;
use std::time::Instant;

static BROWSER: Mutex<Option<BrowserConfig>> = Mutex::new(None);

/// The `[browser]` section the mixer started with.
pub fn configure(browser: BrowserConfig) {
    *BROWSER.lock() = Some(browser);
}

fn lookup() -> web::Lookup {
    web::lookup(&BROWSER.lock().clone().unwrap_or_default())
}

/// Where web pages stand, with nothing running.
pub fn look() -> SetupStatus {
    let l = lookup();
    let base = SetupStatus {
        piece: names::WEB.into(),
        title: names::title(names::WEB).into(),
        detail: l.detail(),
        ..Default::default()
    };
    let (state, refusal) = if l.found.is_some() {
        return SetupStatus { state: SetupState::Ready, message: "Web pages are ready.".into(), ..base };
    } else if l.configured_missing.is_some() {
        (SetupState::Unavailable, plain::web_configured_missing(&l))
    } else if godwinmix_core::probe::exists("wpesrc") {
        return SetupStatus { state: SetupState::Ready, message: "Web pages are ready.".into(), ..base };
    } else if l.buildable.is_some() {
        (SetupState::Missing, plain::web_setting_up(&l))
    } else {
        (SetupState::Unavailable, plain::web_unavailable(&l))
    };
    SetupStatus { state, message: refusal.message, action: Some(refusal.action), ..base }
}

/// Download, unpack, build and bundle, skipping what is done.
pub async fn run(progress: &Progress) -> Result<(), Failure> {
    let l = lookup();
    let Some(dir) = l.buildable.clone() else {
        let refusal = plain::web_unavailable(&l);
        return Err(Failure::new(refusal.message, l.detail()).with_action(refusal.action));
    };
    let log = super::log_path(names::WEB);
    let started = Instant::now();
    build::tools_present(&log)?;
    let engine = cef::Engine::for_checkout(&dir).map_err(|e| build::failed(&log, "engine", e))?;
    if !engine.installed() {
        engine.fetch(progress, &log).await?;
        tracing::info!(secs = started.elapsed().as_secs(), "the web page engine is downloaded and unpacked");
    }
    progress.say(BUILDING, None);
    let built = Instant::now();
    build::cargo_build(&dir, &engine.root, &log).await?;
    build::bundle(&dir, &engine.root, &log).await?;
    tracing::info!(
        build_secs = built.elapsed().as_secs(),
        total_secs = started.elapsed().as_secs(),
        log = %log.display(),
        "the browser renderer is built"
    );
    Ok(())
}

const BUILDING: &str = "Setting up web pages: building the page renderer. This happens once and \
                        takes a few minutes; the page starts by itself when it is ready.";

/// The button under a failure: set up again, carrying on from where it stopped.
fn try_again() -> ErrorAction {
    ErrorAction::setup("Try again", names::WEB)
}
