//! Which mixer a client command talks to, and with what token.
//!
//! In order: what was passed (`--url`, `--token`), the environment
//! (`GODWINMIX_URL`, `GODWINMIX_TOKEN`), the desktop app's own mixer if one is
//! running on this machine, and last `http://127.0.0.1:8080`.
//!
//! The desktop app starts its mixer on a port of its own choosing with a
//! token of its own making, and writes both down in its data folder. Reading
//! them here is what lets an AI agent's configuration be just `godwinmix mcp`,
//! with no address and no secret to copy, for somebody who installed the app
//! and has never opened a terminal.

use godwinmix_core::config;
use std::path::PathBuf;

/// The desktop app's identifier, which names its data folder.
const DESKTOP_APP: &str = "mix.godwin.desktop";

/// The address and token a client command uses.
pub fn resolve(url: Option<String>, token: Option<String>) -> (String, Option<String>) {
    let url = url.or_else(|| config::env_var("URL"));
    let token = token.or_else(|| config::env_var("TOKEN"));
    match url {
        Some(url) => (url, token),
        None => match desktop() {
            Some((url, found)) => (url, token.or(found)),
            None => (crate::DEFAULT_URL.to_string(), token),
        },
    }
}

/// The desktop app's running mixer, when there is one: its address, and the
/// token it was started with.
pub fn desktop() -> Option<(String, Option<String>)> {
    let dir = desktop_data_dir()?;
    let port: u16 = std::fs::read_to_string(dir.join("local-core.port")).ok()?.trim().parse().ok()?;
    let token = std::fs::read_to_string(dir.join("core-token")).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    Some((format!("http://127.0.0.1:{port}"), token))
}

/// Where the desktop app keeps its data, as Tauri lays it out per platform.
pub fn desktop_data_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(windows) {
        env("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library").join("Application Support")
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local").join("share")))?
    };
    Some(base.join(DESKTOP_APP))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_passed_wins_over_anything_found() {
        let (url, token) = resolve(Some("http://mixer:9000".into()), Some("t".into()));
        assert_eq!((url.as_str(), token.as_deref()), ("http://mixer:9000", Some("t")));
    }

    #[test]
    fn the_desktop_apps_folder_is_named_for_the_app() {
        if let Some(dir) = desktop_data_dir() {
            assert!(dir.ends_with(DESKTOP_APP), "{}", dir.display());
        }
    }
}
