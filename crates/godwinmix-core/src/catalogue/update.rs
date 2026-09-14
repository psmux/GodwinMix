//! `gmx codec update` and `gmx codec verify`.
//!
//! 03 section 5: "a catalogue update ships as a tiny release of its own on the
//! same signed channel as plugins, so a driver rename on one platform is fixed
//! without waiting for a core version". This is that channel, and it is
//! deliberately the same code a plugin install uses: the same HTTP, the same
//! sigstore bundle check, the same two levels of answer.
//!
//! ```text
//!   gmx codec update            fetch codecs.toml and its bundle from the release
//!                               channel, check the signature, parse it, and write it
//!                               to ~/.godwinmix/codecs.toml
//!   gmx codec verify <entry>    encode and decode through the entry on this machine,
//!                               append what happened to the entry's `verified` list,
//!                               and print the block to paste into a pull request
//! ```
//!
//! The installed file is an overlay, not a replacement. It is laid over the
//! catalogue compiled into the binary and under the operator's own `[codecs]`
//! table, so an update can rename an element without overriding a choice the
//! operator made on purpose. `load()` in this module's parent is where that
//! order is kept.

use super::model::Verified;
use super::Catalogue;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Where the signed channel is, unless something says otherwise.
///
/// A release of the core repository with two assets on it: `codecs.toml` and
/// `codecs.toml.sigstore.json`. A fork or an organisation that ships its own
/// catalogue sets `GMX_CODEC_CHANNEL` to its own repository.
pub fn default_channel() -> String {
    std::env::var("GMX_CODEC_CHANNEL").unwrap_or_else(|_| "psmux/godwinmix".into())
}

/// Where an installed catalogue update lives.
pub fn installed_path() -> PathBuf {
    if let Ok(explicit) = std::env::var("GMX_CODECS_FILE") {
        return PathBuf::from(explicit);
    }
    godwinmix_host::marketplace::home_dir().join("codecs.toml")
}

/// What an update did.
#[derive(Debug, Clone)]
pub struct Installed {
    pub path: PathBuf,
    /// The release it came from.
    pub tag: String,
    /// What the signature check could say.
    pub trust: String,
    /// The one line explanation under the label.
    pub detail: String,
    pub video: usize,
    pub audio: usize,
    pub graphics: usize,
    /// Entry ids that are in the update and were not in the shipped catalogue.
    pub added: Vec<String>,
    /// Entry ids the update changes.
    pub changed: Vec<String>,
}

/// Fetch, verify and install a catalogue update.
///
/// `channel` is `owner/repo`, or a URL to a directory holding `codecs.toml`
/// and its bundle. A tag pins a particular update; without one the latest
/// release wins.
pub fn install(channel: &str, tag: Option<&str>) -> Result<Installed> {
    let staging = scratch()?;
    let (toml_url, bundle_url, tag) = urls(channel, tag, &staging.dir)?;
    let file = staging.dir.join("codecs.toml");
    godwinmix_host::sources::http::download(&toml_url, &file)
        .with_context(|| format!("fetching the catalogue from {toml_url}"))?;

    let bundle = staging.dir.join("codecs.toml.sigstore.json");
    let (trust, detail) =
        match godwinmix_host::sources::http::download_optional(&bundle_url, &bundle)? {
            Some(_) => {
                let signature =
                    godwinmix_host::verify::check_signature(&file, &bundle, None)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                match signature.level {
                    godwinmix_host::verify::Level::Cosign => (
                        "signed".to_string(),
                        "cosign verified the signature over these bytes".to_string(),
                    ),
                    godwinmix_host::verify::Level::Bundle => (
                        "signed, digest only".to_string(),
                        "the bundle's digest matches these bytes; install cosign for the \
                         full check"
                            .to_string(),
                    ),
                }
            }
            None => anyhow::bail!(
                "there is no codecs.toml.sigstore.json beside {toml_url}. A catalogue \
                 update decides which element encodes your programme, so an unsigned one \
                 is not installed. Put the file at {} by hand if you made it yourself.",
                installed_path().display()
            ),
        };

    // Parse before installing: a catalogue that does not load would take the
    // encoder out from under the next start.
    let fresh = Catalogue::read(&file)?;
    let problems = fresh.validate();
    anyhow::ensure!(
        problems.is_empty(),
        "the catalogue at {toml_url} does not validate and was not installed:\n  {}",
        problems.join("\n  ")
    );
    let shipped = Catalogue::shipped()?;
    let (added, changed) = difference(&shipped, &fresh);

    let target = installed_path();
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("making {}", parent.display()))?;
    }
    std::fs::copy(&file, &target)
        .with_context(|| format!("writing {}", target.display()))?;
    Ok(Installed {
        path: target,
        tag,
        trust,
        detail,
        video: fresh.video.len(),
        audio: fresh.audio.len(),
        graphics: fresh.graphics.len(),
        added,
        changed,
    })
}

/// Which entry ids an update adds, and which it changes.
fn difference(shipped: &Catalogue, fresh: &Catalogue) -> (Vec<String>, Vec<String>) {
    let mut added = Vec::new();
    let mut changed = Vec::new();
    for id in fresh.video.iter().map(|e| e.id()) {
        match shipped.video_entry(&id) {
            None => added.push(id),
            Some(was) => {
                if Some(was) != fresh.video_entry(&id) {
                    changed.push(id);
                }
            }
        }
    }
    for id in fresh.audio.iter().map(|e| e.id()) {
        match shipped.audio_entry(&id) {
            None => added.push(id),
            Some(was) => {
                if Some(was) != fresh.audio_entry(&id) {
                    changed.push(id);
                }
            }
        }
    }
    for id in fresh.graphics.iter().map(|e| e.id()) {
        match shipped.graphics_entry(&id) {
            None => added.push(id),
            Some(was) => {
                if Some(was) != fresh.graphics_entry(&id) {
                    changed.push(id);
                }
            }
        }
    }
    added.sort();
    changed.sort();
    (added, changed)
}

/// Where the catalogue and its bundle are, for a channel written either way.
fn urls(channel: &str, tag: Option<&str>, _scratch: &Path) -> Result<(String, String, String)> {
    if channel.starts_with("http://") || channel.starts_with("https://") {
        let base = channel.trim_end_matches('/').trim_end_matches("/codecs.toml");
        return Ok((
            format!("{base}/codecs.toml"),
            format!("{base}/codecs.toml.sigstore.json"),
            tag.unwrap_or("a URL").to_string(),
        ));
    }
    let parts: Vec<&str> = channel.split('/').collect();
    anyhow::ensure!(
        parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty(),
        "`{channel}` is not a release channel. Write `owner/repo`, or a URL to the \
         directory holding codecs.toml and its signature."
    );
    let api = godwinmix_host::sources::default_github_api();
    let api = api.trim_end_matches('/');
    let path = match tag {
        Some(tag) => format!("{api}/repos/{channel}/releases/tags/{tag}"),
        None => format!("{api}/repos/{channel}/releases/latest"),
    };
    let release = godwinmix_host::sources::http::get_json(&path).with_context(|| {
        format!("asking {channel} for its catalogue release. A channel is a repository with a release carrying codecs.toml")
    })?;
    let found_tag = release["tag_name"].as_str().unwrap_or("?").to_string();
    let asset = |name: &str| -> Option<String> {
        release["assets"].as_array()?.iter().find_map(|a| {
            (a["name"].as_str()? == name)
                .then(|| a["browser_download_url"].as_str().map(str::to_string))?
        })
    };
    let toml_url = asset("codecs.toml").with_context(|| {
        format!(
            "the {found_tag} release of {channel} has no codecs.toml asset. A catalogue \
             update is a release with codecs.toml and codecs.toml.sigstore.json on it."
        )
    })?;
    let bundle_url = asset("codecs.toml.sigstore.json")
        .unwrap_or_else(|| format!("{toml_url}.sigstore.json"));
    Ok((toml_url, bundle_url, found_tag))
}

// ---------------------------------------------------------------------------
// `gmx codec verify`
// ---------------------------------------------------------------------------

/// What a verification appended, and what to paste into a pull request.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub entry: String,
    pub record: Verified,
    /// Where the record was written on this machine.
    pub path: PathBuf,
    /// The TOML block for the pull request against the shipped catalogue.
    pub block: String,
}

/// Append a `verified` record for an entry to the operator's catalogue.
///
/// The machine that ran the test is the only one that knows what happened on
/// it, and 03 section 5 is explicit that this is how the catalogue scales
/// across hardware the project does not own: whoever has the card runs the
/// test and sends the record. Writing it locally as well as printing it means
/// `gmx doctor` can say "this entry has been tested here" on the next run.
pub fn record_verified(
    entry: &str,
    cat: &Catalogue,
    report: &super::check::CodecReport,
    by: &str,
    driver: &str,
) -> Result<Recorded> {
    anyhow::ensure!(
        cat.video_entry(entry).is_some()
            || cat.audio_entry(entry).is_some()
            || cat.graphics_entry(entry).is_some(),
        "`{entry}` is not in the catalogue. `gmx codec list` prints every entry and its id."
    );
    let record = Verified {
        platform: report.platform.clone(),
        driver: driver.to_string(),
        gstreamer: report.gstreamer.clone(),
        by: by.to_string(),
        date: super::check::today(),
        report: report.one_line(),
    };
    let path = installed_path();
    append(&path, entry, &record, cat)?;
    Ok(Recorded {
        entry: entry.to_string(),
        block: block_for(entry, &record, cat),
        record,
        path,
    })
}

/// Add the record to the overlay file, keeping whatever was already there.
///
/// The overlay replaces an entry wholesale, which is the rule the rest of the
/// catalogue follows, so the whole entry is written out with the new record
/// appended to its list rather than a fragment that would half override it.
fn append(path: &Path, entry: &str, record: &Verified, cat: &Catalogue) -> Result<()> {
    let mut overlay = if path.exists() {
        Catalogue::read(path)?
    } else {
        Catalogue::default()
    };
    let mut one = Catalogue::default();
    if let Some(e) = cat.video_entry(entry) {
        let mut e = overlay.video_entry(entry).cloned().unwrap_or_else(|| e.clone());
        e.verified.push(record.clone());
        one.video.push(e);
    } else if let Some(e) = cat.audio_entry(entry) {
        let mut e = overlay.audio_entry(entry).cloned().unwrap_or_else(|| e.clone());
        e.verified.push(record.clone());
        one.audio.push(e);
    } else if let Some(e) = cat.graphics_entry(entry) {
        let mut e = overlay.graphics_entry(entry).cloned().unwrap_or_else(|| e.clone());
        e.verified.push(record.clone());
        one.graphics.push(e);
    }
    overlay.overlay(one);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("making {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(&overlay)
        .context("writing the catalogue overlay back out")?;
    std::fs::write(path, with_header(&text))
        .with_context(|| format!("writing {}", path.display()))
}

fn with_header(body: &str) -> String {
    format!(
        "# Written by `gmx codec update` and `gmx codec verify`.\n\
         #\n\
         # This file is laid over the catalogue built into the binary and under the\n\
         # [codecs] table in your own config, so an entry you set by hand still wins.\n\
         # Delete it to go back to what the core shipped with.\n\n{body}"
    )
}

/// The block a pull request against `codecs.toml` wants.
fn block_for(entry: &str, record: &Verified, cat: &Catalogue) -> String {
    let kind = if cat.video_entry(entry).is_some() {
        "video"
    } else if cat.audio_entry(entry).is_some() {
        "audio"
    } else {
        "graphics"
    };
    format!(
        "# Add to the [[{kind}]] block whose id is {entry}, in codecs.toml:\n\
         verified = [\n  {{ platform = \"{}\", driver = \"{}\", gstreamer = \"{}\", by = \"{}\", date = \"{}\", report = \"{}\" }},\n]\n",
        record.platform, record.driver, record.gstreamer, record.by, record.date, record.report
    )
}

/// Who ran it, when nobody said.
pub fn whoami() -> String {
    std::env::var("GMX_AUTHOR")
        .or_else(|_| std::env::var("USER"))
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

/// What the graphics driver is, as far as this machine will say.
///
/// Three cheap probes and an honest fallback. A record with `unknown` in it is
/// still worth having, because the platform and the GStreamer version are the
/// two fields `gmx doctor` compares against; the driver is what a reader uses
/// to decide whether their own machine is close enough.
pub fn driver() -> String {
    for (program, args) in [
        ("nvidia-smi", &["--query-gpu=driver_version", "--format=csv,noheader"][..]),
        ("sw_vers", &["-productVersion"][..]),
    ] {
        if let Ok(out) = std::process::Command::new(program).args(args).output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !text.is_empty() {
                    let label = if program == "sw_vers" { "macOS " } else { "nvidia " };
                    return format!("{label}{}", text.lines().next().unwrap_or(&text));
                }
            }
        }
    }
    // Linux without an NVIDIA card: the kernel release is the closest thing to
    // a driver version that costs nothing to read.
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| format!("linux {}", s.trim()))
        .unwrap_or_else(|_| "unknown".into())
}

/// A scratch directory that removes itself.
struct Scratch {
    dir: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn scratch() -> Result<Scratch> {
    let dir = std::env::temp_dir().join(format!("gmx-codec-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    Ok(Scratch { dir })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_channel_that_is_not_owner_slash_repo_says_what_one_looks_like() {
        let scratch = std::env::temp_dir();
        let err = urls("godwinmix", None, &scratch).expect_err("one segment is not a channel");
        assert!(format!("{err}").contains("owner/repo"), "{err}");
    }

    #[test]
    fn a_url_channel_takes_the_two_file_names_beside_it() {
        let (toml, bundle, _) =
            urls("https://example.invalid/catalogue/", None, &std::env::temp_dir())
                .expect("a URL channel");
        assert_eq!(toml, "https://example.invalid/catalogue/codecs.toml");
        assert_eq!(bundle, "https://example.invalid/catalogue/codecs.toml.sigstore.json");
    }

    #[test]
    fn the_difference_names_what_an_update_adds_and_what_it_changes() {
        let shipped = Catalogue::parse(
            "[[video]]\ncodec = \"h264\"\naccel = \"software\"\nrank = 64\n\
             encoder = \"openh264enc\"\n",
        )
        .expect("it parses");
        let fresh = Catalogue::parse(
            "[[video]]\ncodec = \"h264\"\naccel = \"software\"\nrank = 90\n\
             encoder = \"openh264enc\"\n\n\
             [[video]]\ncodec = \"av1\"\naccel = \"nvidia\"\nrank = 240\n\
             encoder = \"nvav1enc\"\n",
        )
        .expect("it parses");
        let (added, changed) = difference(&shipped, &fresh);
        assert_eq!(added.len(), 1, "{added:?}");
        assert_eq!(changed.len(), 1, "{changed:?}");
    }

    #[test]
    fn a_verified_record_is_appended_and_the_block_names_the_entry() {
        let cat = Catalogue::shipped().expect("the shipped catalogue parses");
        let entry = cat.video.first().map(|e| e.id()).expect("it has a video entry");
        let record = Verified {
            platform: "linux-x86_64".into(),
            driver: "nvidia 570.86".into(),
            gstreamer: "1.28.0".into(),
            by: "somebody".into(),
            date: "2026-09-14".into(),
            report: "900/900 frames, 0.42 core at real time".into(),
        };
        let block = block_for(&entry, &record, &cat);
        assert!(block.contains(&entry), "{block}");
        assert!(block.contains("nvidia 570.86"), "{block}");
        assert!(block.contains("[[video]]"), "{block}");

        let path = std::env::temp_dir()
            .join(format!("gmx-verify-{}", std::process::id()))
            .join("codecs.toml");
        let _ = std::fs::remove_file(&path);
        append(&path, &entry, &record, &cat).expect("it writes");
        let back = Catalogue::read(&path).expect("it reads back");
        assert_eq!(
            back.video_entry(&entry).map(|e| e.verified.len()),
            Some(1),
            "the record is in the overlay"
        );
        // Twice is two records, not two entries.
        append(&path, &entry, &record, &cat).expect("it writes again");
        let back = Catalogue::read(&path).expect("it reads back");
        assert_eq!(back.video_entry(&entry).map(|e| e.verified.len()), Some(2));
        assert_eq!(back.video.len(), 1);
        let _ = std::fs::remove_dir_all(path.parent().expect("a parent"));
    }
}
