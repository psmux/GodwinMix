//! Where the browser renderer is.
//!
//! Looked for in this order, the first that exists winning:
//!
//! ```text
//!   browser.sidecar                                      the operator's own
//!   <exe dir>/godwinmix-browser[.exe]                    beside the mixer
//!   <exe dir>/godwinmix-browser.app                      the same, on macOS
//!   <exe dir>/browser/godwinmix-browser[.exe]            a packaged folder with its libraries
//!   <exe dir>/../Resources/godwinmix-browser.app         inside a macOS app bundle
//!   <exe dir>/../Resources/browser/godwinmix-browser.app the desktop app's `browser` resource
//!   <exe dir>/../lib/GodwinMix/browser/godwinmix-browser the desktop app's .deb
//!   <exe dir>/../lib/godwinmix/browser/godwinmix-browser an install under a prefix
//!   <checkout>/browser/target/release/...                what the checkout's own build makes
//!   PATH
//! ```
//!
//! On macOS CEF runs only from an app bundle, so there every place names the
//! bundle and the program inside it, and a bare binary in a checkout's
//! target (built, not yet bundled) is not taken. A checkout is the one
//! `plugin::first_party` finds: the folder above the executable with a
//! `Cargo.toml` and a `plugins` folder. It can build the renderer when it
//! also has `browser/Cargo.toml`.

use crate::config::BrowserConfig;
use std::path::{Path, PathBuf};

pub const NAME: &str = "godwinmix-browser";

/// What a look found.
#[derive(Debug, Clone, Default)]
pub struct Lookup {
    /// The program to run, when there is one.
    pub found: Option<PathBuf>,
    /// Every place looked, in order, for the detail of a refusal.
    pub looked: Vec<PathBuf>,
    /// The checkout's `browser` folder, when this mixer can build it there.
    pub buildable: Option<PathBuf>,
    /// The operator named a path that is not there.
    pub configured_missing: Option<PathBuf>,
}

impl Lookup {
    /// For `data.detail`: where it looked and what it would build.
    pub fn detail(&self) -> serde_json::Value {
        serde_json::json!({
            "program": NAME,
            "looked": self.looked.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "build_in": self.buildable.as_ref().map(|p| p.display().to_string()),
            "setting": "browser.sidecar",
        })
    }
}

/// Look for the renderer the way the mixer running now would.
pub fn lookup(browser: &BrowserConfig) -> Lookup {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let path = std::env::var_os("PATH");
    lookup_from(browser, exe_dir.as_deref(), path.as_deref())
}

pub fn lookup_from(browser: &BrowserConfig, exe_dir: Option<&Path>, path: Option<&std::ffi::OsStr>) -> Lookup {
    let mut out = Lookup::default();
    if let Some(p) = browser.sidecar.as_deref().filter(|p| !p.trim().is_empty()) {
        let p = program_in(Path::new(p));
        if p.is_file() {
            out.found = Some(p);
        } else {
            out.looked.push(p.clone());
            out.configured_missing = Some(p);
        }
        return out;
    }
    let checkout = exe_dir.and_then(crate::plugin::first_party::checkout_of);
    let mut places: Vec<PathBuf> = exe_dir.map(packaged_places).unwrap_or_default();
    if let Some(c) = &checkout {
        places.push(checkout_place(c));
    }
    if let Some(path) = path {
        places.extend(std::env::split_paths(path).map(|d| d.join(exe_name())));
    }
    out.found = places.iter().find(|p| p.is_file()).cloned();
    out.looked = places;
    out.buildable = checkout.map(|c| c.join("browser")).filter(|b| b.join("Cargo.toml").is_file());
    out
}

/// `godwinmix-browser.app` names the program inside it; anything else is
/// taken as the program.
fn program_in(p: &Path) -> PathBuf {
    if p.extension().is_some_and(|e| e == "app") {
        return p.join("Contents").join("MacOS").join(NAME);
    }
    p.to_path_buf()
}

fn exe_name() -> String {
    format!("{NAME}{}", std::env::consts::EXE_SUFFIX)
}

fn bundle_program(dir: &Path) -> PathBuf {
    dir.join(format!("{NAME}.app")).join("Contents").join("MacOS").join(NAME)
}

/// The places a package puts it, relative to the mixer's own folder.
fn packaged_places(exe_dir: &Path) -> Vec<PathBuf> {
    let prefix = exe_dir.parent().unwrap_or(exe_dir);
    if cfg!(target_os = "macos") {
        return vec![
            bundle_program(exe_dir),
            bundle_program(&prefix.join("Resources")),
            bundle_program(&prefix.join("Resources").join("browser")),
            bundle_program(&prefix.join("lib").join("godwinmix")),
        ];
    }
    // Tauri puts a resource beside the executable on Windows and under
    // `/usr/lib/<product name>` in a .deb.
    vec![
        exe_dir.join(exe_name()),
        exe_dir.join("browser").join(exe_name()),
        prefix.join("lib").join("godwinmix").join("browser").join(exe_name()),
        prefix.join("lib").join("GodwinMix").join("browser").join(exe_name()),
    ]
}

/// Where the checkout's own build leaves it: `cargo build --release` in
/// `browser/`, then on macOS `browser/dev/mac-bundle.sh`.
pub fn checkout_place(checkout: &Path) -> PathBuf {
    let release = checkout.join("browser").join("target").join("release");
    if cfg!(target_os = "macos") {
        bundle_program(&release)
    } else {
        release.join(exe_name())
    }
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;
