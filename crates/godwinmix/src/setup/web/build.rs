//! Unpacking the engine, building the renderer, and bundling it on macOS.
//!
//! Every command runs as a child process with its output in the setup log
//! (`super::super::run`). What only the operating system can supply (the
//! build tools CEF's wrapper needs) is said with the one command to install
//! them, which the page shows with a copy button.

use super::super::run::{logged, on_path};
use super::super::Failure;
use godwinmix_core::setup::names;
use godwinmix_protocol::ErrorAction;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// A step that did not finish, in a person's words, with the log named in
/// the detail.
pub fn failed(log: &Path, step: &str, error: String) -> Failure {
    tracing::warn!(step, %error, log = %log.display(), "setting up web pages stopped");
    Failure::new(
        "Web pages could not be set up this time. Press Try again; if it stops again, the setup \
         log in the mixer's folder says why.",
        json!({ "step": step, "error": error, "log": log }),
    )
    .with_action(super::try_again())
}

/// The command that installs CMake and Ninja here.
fn tools_command() -> &'static str {
    if cfg!(target_os = "macos") {
        "brew install cmake ninja"
    } else if cfg!(target_os = "windows") {
        "winget install Kitware.CMake Ninja-build.Ninja"
    } else if Path::new("/usr/bin/dnf").exists() {
        "sudo dnf install cmake ninja-build"
    } else {
        "sudo apt install cmake ninja-build"
    }
}

/// CMake and Ninja, which the engine's wrapper library is built with.
pub fn tools_present(log: &Path) -> Result<(), Failure> {
    let missing: Vec<&str> = ["cmake", "ninja"].into_iter().filter(|t| on_path(t).is_none()).collect();
    if missing.is_empty() {
        return Ok(());
    }
    let command = tools_command();
    Err(Failure::new(
        "Setting up web pages needs two build tools this machine does not have. Install them with \
         the command below, then press Try again.",
        json!({ "missing": missing, "command": command, "log": log }),
    )
    .with_action(ErrorAction::setup("Try again", names::WEB).with_command(command)))
}

fn cargo() -> PathBuf {
    if let Some(c) = std::env::var_os("CARGO").map(PathBuf::from).filter(|c| c.is_file()) {
        return c;
    }
    on_path("cargo").unwrap_or_else(|| {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).unwrap_or_default();
        PathBuf::from(home).join(".cargo").join("bin").join(format!("cargo{}", std::env::consts::EXE_SUFFIX))
    })
}

/// The environment every build here runs with: the engine it is to use,
/// patience with crates.io, and the browser's own target folder.
fn prepare(cmd: &mut Command, engine_root: &Path) {
    cmd.env("CEF_PATH", engine_root).env("CARGO_NET_RETRY", "10").env_remove("CARGO_TARGET_DIR");
    // `cargo run` hands its child the mixer's own package variables; the
    // renderer is another package and must not see them.
    for (k, _) in std::env::vars_os() {
        if k.to_str().is_some_and(|k| k.starts_with("CARGO_PKG_") || k == "CARGO_MANIFEST_DIR") {
            cmd.env_remove(k);
        }
    }
}

/// `cargo build --release` in `browser/`.
pub async fn cargo_build(browser: &Path, engine_root: &Path, log: &Path) -> Result<(), Failure> {
    let mut cmd = Command::new(cargo());
    cmd.arg("build").arg("--release").current_dir(browser);
    prepare(&mut cmd, engine_root);
    logged(cmd, log).await.map_err(|e| failed(log, "build", e))
}

/// On macOS, the app bundle CEF runs from. Elsewhere the build is the program.
pub async fn bundle(browser: &Path, engine_root: &Path, log: &Path) -> Result<(), Failure> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    let mut cmd = Command::new("/bin/bash");
    cmd.arg(browser.join("dev").join("mac-bundle.sh")).current_dir(browser);
    prepare(&mut cmd, engine_root);
    logged(cmd, log).await.map_err(|e| failed(log, "bundle", e))
}

/// Unpack the archive into the layout the engine's own downloader leaves:
/// `Release` as the folder, the build files beside it, and on Linux and
/// Windows the resources too; then the `archive.json` the build checks.
pub fn unpack(archive: &Path, versioned: &Path, os_arch: &str, record: &Value, log: &Path) -> Result<(), String> {
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    let scratch = versioned.join("unpacking");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let out = std::fs::OpenOptions::new().create(true).append(true).open(log).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("tar")
        .arg("-xjf")
        .arg(archive)
        .arg("-C")
        .arg(&scratch)
        .stdout(out.try_clone().map_err(|e| e.to_string())?)
        .stderr(out)
        .status()
        .map_err(|e| format!("starting tar: {e}"))?;
    if !status.success() {
        return Err(format!("tar ended with {status}"));
    }
    let top = std::fs::read_dir(&scratch)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.is_dir())
        .ok_or("the archive held no folder")?;
    let dest = versioned.join(os_arch);
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(top.join("Release"), &dest).map_err(|e| format!("moving Release: {e}"))?;
    if !cfg!(target_os = "macos") {
        for entry in std::fs::read_dir(top.join("Resources")).map_err(|e| e.to_string())?.flatten() {
            std::fs::rename(entry.path(), dest.join(entry.file_name())).map_err(|e| e.to_string())?;
        }
    }
    for part in ["CMakeLists.txt", "cmake", "include", "libcef_dll", "CREDITS.html"] {
        std::fs::rename(top.join(part), dest.join(part)).map_err(|e| format!("moving {part}: {e}"))?;
    }
    let text = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    std::fs::write(dest.join("archive.json"), text).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&scratch);
    let _ = std::fs::remove_file(archive);
    Ok(())
}
