//! Where the web page engine is downloaded from.
//!
//! GodwinMix's own builds of CEF play H.264 and AAC; the official ones do not.
//! Ours are releases of psmux/godwinmix-cef, one per CEF version, each with an
//! `index.json` like the official one, so both are read the same way. Ours is
//! asked first, and the official one serves a platform ours has not reached.

use super::download;
use serde_json::{json, Value};

/// Where to look, in order: `CEF_DOWNLOAD_URL` alone if set, else ours for
/// `version`, then the official CDN.
pub fn cdns(version: &str) -> Vec<String> {
    match std::env::var("CEF_DOWNLOAD_URL") {
        Ok(url) => vec![url],
        Err(_) => vec![
            format!("https://github.com/psmux/godwinmix-cef/releases/download/cef-{version}"),
            "https://cef-builds.spotifycdn.com".into(),
        ],
    }
}

/// The first place with a minimal archive of `version` for `key`, and that
/// archive's entry. On failure, what each place answered.
pub async fn find(client: &reqwest::Client, key: &str, version: &str) -> Result<(String, Value), Vec<Value>> {
    let mut tried = Vec::new();
    for cdn in cdns(version) {
        let url = format!("{cdn}/index.json");
        // A release that does not exist is a quick 404, not worth the retries.
        let missing = client.get(&url).send().await.is_ok_and(|r| r.status() == reqwest::StatusCode::NOT_FOUND);
        let index = if missing { Err("not found".to_string()) } else { download::json(client, &url).await };
        match index.map(|index| minimal(&index, key, version)) {
            Ok(Some(file)) => return Ok((cdn, file)),
            Ok(None) => tried.push(json!({ "url": url, "error": format!("no minimal {version} for {key}") })),
            Err(e) => tried.push(json!({ "url": url, "error": e })),
        }
    }
    Err(tried)
}

/// The minimal archive of `version` for `key` in an index like the official one.
pub fn minimal(index: &Value, key: &str, version: &str) -> Option<Value> {
    let prefix = format!("{version}+");
    index[key]["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| v["cef_version"].as_str().is_some_and(|c| c.starts_with(&prefix)))
        .flat_map(|v| v["files"].as_array().cloned().unwrap_or_default())
        .find(|f| f["type"] == "minimal")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn ours_is_asked_before_the_official_cdn() {
        if std::env::var_os("CEF_DOWNLOAD_URL").is_none() {
            let cdns = super::cdns("150.0.10");
            assert_eq!(cdns[0], "https://github.com/psmux/godwinmix-cef/releases/download/cef-150.0.10");
            assert_eq!(cdns[1], "https://cef-builds.spotifycdn.com");
        }
    }

    #[test]
    fn the_minimal_archive_of_the_pinned_version_is_found() {
        let file = json!({ "type": "minimal", "name": "cef_binary_150.0.10+g1+chromium-1_windows64_minimal.tar.bz2" });
        let index = json!({ "windows64": { "versions": [
            { "cef_version": "150.0.1+g0+chromium-1", "files": [{ "type": "minimal", "name": "old" }] },
            { "cef_version": "150.0.10+g1+chromium-1", "files": [{ "type": "standard", "name": "big" }, file] },
        ] } });
        assert_eq!(super::minimal(&index, "windows64", "150.0.10"), Some(file));
        assert_eq!(super::minimal(&index, "linux64", "150.0.10"), None);
    }
}
