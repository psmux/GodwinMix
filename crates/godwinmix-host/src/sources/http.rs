//! The little bit of HTTP a plugin install needs.
//!
//! Blocking on purpose. Installing a plugin already runs on a blocking task
//! (the loader copies trees and shells out to package managers there), and an
//! async client would put a second runtime in a crate that has none. The
//! whole surface is three functions.

use anyhow::{Context, Result};
use super::Progress;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

/// How long any one request may take before it is given up on. A release asset
/// on a slow link is the long case; ten minutes is generous and finite.
pub const TIMEOUT: Duration = Duration::from_secs(600);

fn client() -> Result<reqwest::blocking::Client> {
    client_within(TIMEOUT)
}

fn client_within(timeout: Duration) -> Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(concat!("godwinmix/", env!("CARGO_PKG_VERSION")))
        .timeout(timeout);
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
    get_json_within(url, TIMEOUT)
}

/// GET a JSON document, giving up after `timeout`. The gallery asks with a
/// few seconds, because a person is looking at the page while it waits.
pub fn get_json_within(url: &str, timeout: Duration) -> Result<serde_json::Value> {
    let mut request = client_within(timeout)?.get(url).header("Accept", "application/vnd.github+json");
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
    download_with(url, to, None)
}

/// [`download`], telling `progress` how much has arrived as it goes.
pub fn download_with(url: &str, to: &Path, progress: Option<&Progress>) -> Result<u64> {
    match fetch_to(url, to, progress)? {
        Some(bytes) => Ok(bytes),
        None => anyhow::bail!("{url} answered 404, so there is nothing to download there."),
    }
}

/// The same, but a 404 is an answer rather than a failure. That is how a
/// missing signature file is told from a broken network.
pub fn download_optional(url: &str, to: &Path) -> Result<Option<u64>> {
    fetch_to(url, to, None)
}

fn fetch_to(url: &str, to: &Path, progress: Option<&Progress>) -> Result<Option<u64>> {
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
    let size = response.content_length();
    let bytes = copy_counting(&mut response, &mut writer, size, progress)
        .with_context(|| format!("saving {url} to {}", to.display()))?;
    writer.flush().with_context(|| format!("finishing {}", to.display()))?;
    Ok(Some(bytes))
}

/// Copy a body to a file, saying how far along it is after every read.
fn copy_counting(
    from: &mut impl Read,
    to: &mut impl Write,
    size: Option<u64>,
    progress: Option<&Progress>,
) -> std::io::Result<u64> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    if let Some(tell) = progress {
        (tell.0)(0, size);
    }
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => return Ok(done),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        to.write_all(&buf[..n])?;
        done += n as u64;
        if let Some(tell) = progress {
            (tell.0)(done, size);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_copy_says_how_far_along_it_is_against_the_size_given() {
        let body = vec![7u8; 150 * 1024];
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let tell = Progress(Arc::new(move |done, size| log.lock().unwrap().push((done, size))));
        let mut out = Vec::new();
        let copied = copy_counting(&mut body.as_slice(), &mut out, Some(150 * 1024), Some(&tell))
            .expect("it copies");
        assert_eq!(copied, 150 * 1024);
        assert_eq!(out, body);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.first(), Some(&(0, Some(150 * 1024))));
        assert_eq!(seen.last(), Some(&(150 * 1024, Some(150 * 1024))));
        assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "it only goes forward: {seen:?}");
    }
}
