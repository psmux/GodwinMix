//! The little bit of HTTP a plugin install needs.
//!
//! Blocking on purpose. Installing a plugin already runs on a blocking task
//! (the loader copies trees and shells out to package managers there), and an
//! async client would put a second runtime in a crate that has none. The
//! whole surface is three functions.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// How long any one request may take before it is given up on. A release asset
/// on a slow link is the long case; ten minutes is generous and finite.
const TIMEOUT: Duration = Duration::from_secs(600);

fn client() -> Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(concat!("godwinmix/", env!("CARGO_PKG_VERSION")))
        .timeout(TIMEOUT);
    // An operator behind a proxy that only speaks HTTP/1.1 is common enough on
    // a broadcast network to be worth not surprising.
    builder = builder.redirect(reqwest::redirect::Policy::limited(10));
    builder.build().context("building the HTTP client")
}

/// A token for the GitHub API, when one is set. Unauthenticated calls are
/// limited to 60 an hour per address, which a CI runner exhausts in a morning.
fn github_token() -> Option<String> {
    for key in ["GMX_GITHUB_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// GET a JSON document.
pub fn get_json(url: &str) -> Result<serde_json::Value> {
    let mut request = client()?.get(url).header("Accept", "application/vnd.github+json");
    if url.contains("github") {
        if let Some(token) = github_token() {
            request = request.bearer_auth(token);
        }
    }
    let response = request.send().with_context(|| format!("asking {url}"))?;
    let status = response.status();
    let body = response.text().unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("{url} answered {status}: {}", first_words(&body));
    }
    serde_json::from_str(&body).with_context(|| format!("{url} did not answer with JSON"))
}

/// GET a file to disk. Answers with how many bytes arrived.
pub fn download(url: &str, to: &Path) -> Result<u64> {
    match download_optional(url, to)? {
        Some(bytes) => Ok(bytes),
        None => anyhow::bail!("{url} answered 404, so there is nothing to download there."),
    }
}

/// The same, but a 404 is an answer rather than a failure. That is how a
/// missing signature file is told from a broken network.
pub fn download_optional(url: &str, to: &Path) -> Result<Option<u64>> {
    let mut request = client()?.get(url).header("Accept", "application/octet-stream");
    if url.contains("github") {
        if let Some(token) = github_token() {
            request = request.bearer_auth(token);
        }
    }
    let mut response = request.send().with_context(|| format!("fetching {url}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        anyhow::bail!("{url} answered {status}: {}", first_words(&body));
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("making {}", parent.display()))?;
    }
    let file = std::fs::File::create(to)
        .with_context(|| format!("writing {}", to.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    let bytes = response
        .copy_to(&mut writer)
        .with_context(|| format!("saving {url} to {}", to.display()))?;
    writer.flush().with_context(|| format!("finishing {}", to.display()))?;
    Ok(Some(bytes))
}

/// The first line of an error body, trimmed. A GitHub error is JSON with a
/// `message`; anything else gets its first eighty characters.
fn first_words(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = value.get("message").and_then(serde_json::Value::as_str) {
            return message.to_string();
        }
    }
    let line = body.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    line.chars().take(80).collect()
}
