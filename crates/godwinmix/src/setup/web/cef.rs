//! The web page engine the renderer is built on: which one, and getting it.
//!
//! The renderer's build (the `cef-dll-sys` crate) downloads CEF itself, once,
//! with no retry and nothing said while it runs. So the mixer does that part
//! first, its own way, and leaves the engine exactly where that build looks
//! (`$CEF_PATH/<version>/cef_<os>_<arch>`, with the `archive.json` it
//! checks); the build then finds it and downloads nothing.

use super::super::registry::Progress;
use super::super::Failure;
use super::download;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Where the builds of the renderer look for engines, and which one.
pub struct Engine {
    /// `CEF_PATH`, else `~/.cache/gmx-cef`: the place `browser/dev` uses.
    pub root: PathBuf,
    /// `150.0.10`: the `+metadata` of the `cef-dll-sys` the checkout pins.
    pub version: String,
}

/// The CDN the build itself downloads from, unless `CEF_DOWNLOAD_URL` says.
fn cdn() -> String {
    std::env::var("CEF_DOWNLOAD_URL").unwrap_or_else(|_| "https://cef-builds.spotifycdn.com".into())
}

/// This machine as the engine's index names it, and as its folder does.
pub fn platform() -> Option<(&'static str, String)> {
    use std::env::consts::{ARCH, OS};
    let key = match (OS, ARCH) {
        ("macos", "aarch64") => "macosarm64",
        ("macos", "x86_64") => "macosx64",
        ("linux", "x86_64") => "linux64",
        ("linux", "aarch64") => "linuxarm64",
        ("windows", "x86_64") => "windows64",
        ("windows", "aarch64") => "windowsarm64",
        _ => return None,
    };
    Some((key, format!("cef_{OS}_{ARCH}")))
}

/// The engine version a checkout's renderer is pinned to, from its lock file.
pub fn pinned(lock: &str) -> Option<String> {
    let at = lock.find("name = \"cef-dll-sys\"")?;
    let line = lock[at..].lines().nth(1)?;
    let version = line.strip_prefix("version = \"")?.trim_end_matches('"');
    version.split_once('+').map(|(_, v)| v.to_string())
}

impl Engine {
    pub fn for_checkout(browser: &Path) -> Result<Self, String> {
        let lock = std::fs::read_to_string(browser.join("Cargo.lock"))
            .map_err(|e| format!("reading {}: {e}", browser.join("Cargo.lock").display()))?;
        let version = pinned(&lock).ok_or("no cef-dll-sys with a +version in browser/Cargo.lock")?;
        let root = std::env::var_os("CEF_PATH").map(PathBuf::from).unwrap_or_else(default_root);
        Ok(Self { root, version })
    }

    fn dir(&self) -> Option<PathBuf> {
        Some(self.root.join(&self.version).join(platform()?.1))
    }

    /// Unpacked and marked, so the build will take it as it is.
    pub fn installed(&self) -> bool {
        self.dir().is_some_and(|d| d.join("archive.json").is_file())
    }

    /// Index, archive, digest, unpack. A failure says the download did not
    /// finish and offers to try again; what is downloaded is kept for it.
    pub async fn fetch(&self, progress: &Progress, log: &Path) -> Result<(), Failure> {
        progress.say("Setting up web pages: downloading the page engine. This happens once.", Some(0.0));
        let client = download::client().map_err(|e| not_downloaded(json!({ "error": e })))?;
        let file = self.find(&client).await?;
        let name = file["name"].as_str().unwrap_or_default().to_string();
        let size = file["size"].as_u64().unwrap_or(0);
        let sha1 = file["sha1"].as_str().unwrap_or_default().to_string();
        let versioned = self.root.join(&self.version);
        std::fs::create_dir_all(&versioned).map_err(|e| not_downloaded(json!({ "error": e.to_string() })))?;
        let part = versioned.join(format!("{name}.part"));
        let url = format!("{}/{name}", cdn());
        let mb = |b: u64| b / 1_000_000;
        let say = |got: u64| {
            let text = format!(
                "Setting up web pages: downloading the page engine ({} of {} MB). This happens once.",
                mb(got),
                mb(size)
            );
            progress.say(&text, Some(got as f64 / size.max(1) as f64));
        };
        download::resume(&client, &url, &part, size, say)
            .await
            .map_err(|e| not_downloaded(json!({ "url": url, "error": e, "part": part })))?;
        if super::check::digest(&part).await != sha1 {
            let _ = std::fs::remove_file(&part);
            return Err(not_downloaded(json!({ "url": url, "error": "the digest did not match; removed to download again" })));
        }
        progress.say("Setting up web pages: unpacking the page engine.", None);
        let (dest, os_arch) = (versioned.clone(), platform().map(|p| p.1).unwrap_or_default());
        let record = json!({ "type": "minimal", "name": name, "sha1": sha1 });
        let log = log.to_path_buf();
        let unpacked = tokio::task::spawn_blocking(move || super::build::unpack(&part, &dest, &os_arch, &record, &log))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        unpacked.map_err(|e| super::build::failed(&super::super::log_path("web"), "unpack", e))
    }

    /// The minimal archive for this version and machine, from the index.
    async fn find(&self, client: &reqwest::Client) -> Result<Value, Failure> {
        let (key, _) = platform().ok_or_else(|| not_downloaded(json!({ "error": "no engine for this machine" })))?;
        let url = format!("{}/index.json", cdn());
        let index = download::json(client, &url).await.map_err(|e| not_downloaded(json!({ "url": url, "error": e })))?;
        let prefix = format!("{}+", self.version);
        let found = index[key]["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["cef_version"].as_str().is_some_and(|c| c.starts_with(&prefix)))
            .flat_map(|v| v["files"].as_array().cloned().unwrap_or_default())
            .find(|f| f["type"] == "minimal");
        found.ok_or_else(|| {
            not_downloaded(json!({ "url": url, "error": format!("no minimal {} for {key} in the index", self.version) }))
        })
    }
}

fn default_root() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    home.unwrap_or_else(std::env::temp_dir).join(".cache").join("gmx-cef")
}

/// The download did not finish, in a person's words.
pub fn not_downloaded(detail: Value) -> Failure {
    tracing::warn!(%detail, "the web page engine download did not finish");
    Failure::new(
        "Web pages are not set up yet: the download did not finish. Check the internet connection \
         and press Try again; it carries on from where it stopped.",
        detail,
    )
    .with_action(super::try_again())
}

#[cfg(test)]
#[path = "cef_tests.rs"]
mod tests;
