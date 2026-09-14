//! `gmx plugin add owner/gmx-ndi`: a GitHub release, one asset per platform.
//!
//! This is the default form and the one the index CI produces. A release has
//! an asset whose name carries the platform triple and, beside it, a sigstore
//! bundle with the same name plus `.sigstore.json`. Nothing else about the
//! repository is read: no branch, no source tree, no build. An author who
//! wants their plugin built here instead publishes a git source with a
//! `[build]` section.

use super::{archive, http, FetchCtx, Fetched, Source};
use crate::verify::{self, Trust};
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Every platform triple `launch::this_platform` can answer, so a refusal can
/// say which of them a release does have.
const TRIPLES: &[&str] = &[
    "linux-x86_64",
    "linux-aarch64",
    "linux-armv7",
    "macos-aarch64",
    "macos-x86_64",
    "windows-x86_64",
    "windows-aarch64",
];

/// Turn a GitHub web URL into the `owner/repo` form, so a pasted address works.
pub fn from_url(url: &str) -> Option<Source> {
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("https://www.github.com/"))?;
    let mut parts = rest.trim_end_matches('/').split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(Source::GitHub { owner, repo, version: None })
}

pub fn fetch(
    owner: &str,
    repo: &str,
    version: Option<&str>,
    ctx: &FetchCtx,
) -> Result<Fetched> {
    let release = release(owner, repo, version, ctx)?;
    let tag = release["tag_name"].as_str().unwrap_or("?").to_string();
    let assets = assets(&release);
    let names: Vec<String> = assets.iter().map(|(n, _)| n.clone()).collect();
    let asset = archive::asset_for(&names, &ctx.platform).ok_or_else(|| {
        anyhow::anyhow!(
            "{owner}/{repo} {tag} has no asset for {}. It has: {}. Ask the author to add \
             this platform to their CI matrix, or install from the git source with \
             `gmx plugin add https://github.com/{owner}/{repo}.git`, which builds here.",
            ctx.platform,
            platforms_in(&names)
        )
    })?;
    let url = assets
        .iter()
        .find(|(n, _)| *n == asset)
        .map(|(_, u)| u.clone())
        .expect("the asset was chosen from this list");

    let downloads = ctx.staging.join("download");
    let file = downloads.join(&asset);
    let bytes = http::download(&url, &file)?;
    let mut notes =
        vec![format!("{owner}/{repo} {tag}: {asset} ({} KB)", bytes / 1024)];

    let origin = format!("{owner}/{repo}@{tag}");
    let trust = match archive::signature_for(&names, &asset) {
        Some(sig_name) => {
            let sig_url = assets
                .iter()
                .find(|(n, _)| *n == sig_name)
                .map(|(_, u)| u.clone())
                .expect("the signature was chosen from this list");
            let sig_file = downloads.join(&sig_name);
            http::download(&sig_url, &sig_file)?;
            let signature = verify::check_signature(&file, &sig_file, ctx.identity.as_ref())
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            if signature.level == verify::Level::Bundle {
                notes.push(
                    "the bundle's digest matches these bytes, but cosign is not installed so \
                     nothing checked who signed it. `brew install cosign` or the release from \
                     sigstore/cosign gets the full check."
                        .into(),
                );
            } else {
                notes.push("cosign verified the signature".into());
            }
            Trust::signed(&origin, signature)
        }
        None => Trust::unsigned(
            &origin,
            format!("{tag} has no {asset}.sigstore.json beside the asset"),
        ),
    };

    let unpacked = ctx.staging.join("unpacked");
    archive::unpack(&file, &unpacked)?;
    let dir = super::find_manifest_root(&unpacked)?;
    Ok(Fetched { dir, trust, notes })
}

/// The release JSON, by tag when one was asked for and the latest otherwise.
fn release(
    owner: &str,
    repo: &str,
    version: Option<&str>,
    ctx: &FetchCtx,
) -> Result<serde_json::Value> {
    let base = ctx.github_api.trim_end_matches('/');
    let Some(version) = version else {
        return http::get_json(&format!("{base}/repos/{owner}/{repo}/releases/latest"))
            .with_context(|| {
                format!(
                    "{owner}/{repo} has no published release. A plugin is installed from a \
                     release asset; if the author has not tagged one, install from the git \
                     source instead: gmx plugin add https://github.com/{owner}/{repo}.git"
                )
            });
    };
    // `v1.2.0` is what almost everyone tags; `1.2.0` is what the rest do.
    let tagged = version.strip_prefix('v').unwrap_or(version);
    let mut last = None;
    for tag in [format!("v{tagged}"), tagged.to_string()] {
        match http::get_json(&format!("{base}/repos/{owner}/{repo}/releases/tags/{tag}")) {
            Ok(found) => return Ok(found),
            Err(e) => last = Some(e),
        }
    }
    Err(anyhow::anyhow!(
        "{owner}/{repo} has no release tagged v{tagged} or {tagged}. \
         `gmx plugin search {repo}` lists the versions an index knows about. \
         The last answer was: {}",
        last.map(|e| format!("{e}")).unwrap_or_default()
    ))
}

/// `(name, download url)` for every asset on a release.
fn assets(release: &serde_json::Value) -> Vec<(String, String)> {
    release["assets"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|a| {
                    let name = a["name"].as_str()?.to_string();
                    let url = a["browser_download_url"]
                        .as_str()
                        .or_else(|| a["url"].as_str())?
                        .to_string();
                    Some((name, url))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Which platforms a release does carry, for the refusal.
fn platforms_in(names: &[String]) -> String {
    let mut found: Vec<&str> = TRIPLES
        .iter()
        .filter(|t| names.iter().any(|n| n.to_ascii_lowercase().contains(**t)))
        .copied()
        .collect();
    found.sort_unstable();
    if found.is_empty() {
        format!(
            "no asset with a platform triple in its name ({} assets in all)",
            names.len()
        )
    } else {
        found.join(", ")
    }
}

/// Where a fetch's downloads go, for a caller that wants to look at them.
pub fn downloads_dir(staging: &std::path::Path) -> PathBuf {
    staging.join("download")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_github_url_becomes_owner_and_repo() {
        assert_eq!(
            from_url("https://github.com/psmux/gmx-ndi"),
            Some(Source::GitHub {
                owner: "psmux".into(),
                repo: "gmx-ndi".into(),
                version: None
            })
        );
        assert_eq!(
            from_url("https://github.com/psmux/gmx-ndi/"),
            Some(Source::GitHub {
                owner: "psmux".into(),
                repo: "gmx-ndi".into(),
                version: None
            })
        );
        assert_eq!(from_url("https://gitlab.com/x/y"), None);
    }

    #[test]
    fn the_refusal_lists_the_platforms_the_release_has() {
        let names: Vec<String> = ["a-linux-x86_64.tar.gz", "a-macos-aarch64.tar.gz"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let listed = platforms_in(&names);
        assert!(listed.contains("linux-x86_64"), "{listed}");
        assert!(listed.contains("macos-aarch64"), "{listed}");
        assert!(!listed.contains("windows"), "{listed}");
    }

    #[test]
    fn assets_are_read_off_the_release_json() {
        let release = serde_json::json!({
            "tag_name": "v1.0.0",
            "assets": [
                { "name": "a.tar.gz", "browser_download_url": "http://x/a.tar.gz" },
                { "name": "b.tar.gz" }
            ]
        });
        let found = assets(&release);
        assert_eq!(found.len(), 1, "an asset with no URL is not usable");
        assert_eq!(found[0].0, "a.tar.gz");
    }
}
