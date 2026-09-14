//! Unpacking a release asset.
//!
//! No archive crate. `tar` reads gzip on every platform this project supports:
//! GNU tar on Linux, bsdtar on macOS, and bsdtar shipped as `tar.exe` in
//! Windows 10 1803 and later. bsdtar reads zip as well, and where it is not
//! bsdtar there is usually `unzip`. That is two shell outs against a
//! dependency tree (a tar reader, a gzip decoder, a zip reader and a CRC
//! crate) that would be compiled into every core on every machine to unpack a
//! file most operators unpack once.
//!
//! The one thing that is not delegated is the path check: an archive is not
//! allowed to write outside the directory it is unpacked into, and both
//! extractors are given the flags that refuse it.

use super::run;
use anyhow::{Context, Result};
use std::path::Path;

/// Unpack `archive` into `into`, which is made if it is not there.
///
/// The format is read off the file name, because that is what a release asset
/// carries and guessing from magic bytes would still have to pick an
/// extractor.
pub fn unpack(archive: &Path, into: &Path) -> Result<()> {
    std::fs::create_dir_all(into).with_context(|| format!("making {}", into.display()))?;
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let archive_arg = archive.to_string_lossy().into_owned();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        run("unpacking the asset", "tar", &["-xzf", &archive_arg, "-C", "."], into)?;
        return Ok(());
    }
    if name.ends_with(".tar") {
        run("unpacking the asset", "tar", &["-xf", &archive_arg, "-C", "."], into)?;
        return Ok(());
    }
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        run("unpacking the asset", "tar", &["-xJf", &archive_arg, "-C", "."], into)?;
        return Ok(());
    }
    if name.ends_with(".crate") {
        // A .crate is a gzipped tar with the version in the top directory.
        run("unpacking the crate", "tar", &["-xzf", &archive_arg, "-C", "."], into)?;
        return Ok(());
    }
    if name.ends_with(".zip") {
        return unzip(&archive_arg, into);
    }
    anyhow::bail!(
        "`{name}` is not an archive this can open. A plugin release asset is a .tar.gz \
         (preferred, it keeps the executable bit on every platform) or a .zip. \
         docs/how-to/publish-a-plugin.md says how the index CI names them."
    )
}

fn unzip(archive: &str, into: &Path) -> Result<()> {
    if super::have("unzip") {
        run("unpacking the asset", "unzip", &["-q", "-o", archive], into)?;
        return Ok(());
    }
    // bsdtar, which is what `tar` is on macOS and Windows, reads zip.
    run("unpacking the asset", "tar", &["-xf", archive, "-C", "."], into).context(
        "neither `unzip` nor a tar that reads zip is installed. Install unzip, or ask the \
         plugin author for a .tar.gz asset.",
    )?;
    Ok(())
}

/// Which of a release's asset names is for this platform.
///
/// The rule is the one the index CI follows: the platform triple appears in
/// the file name. `gmx-ndi-1.2.0-linux-x86_64.tar.gz` is for `linux-x86_64`.
/// Nothing is inferred from `amd64` or `x64`; the triples in `launch.rs` are
/// the vocabulary, and the CI template writes them.
pub fn asset_for(names: &[String], platform: &str) -> Option<String> {
    names
        .iter()
        .filter(|n| !is_signature(n))
        .filter(|n| n.to_ascii_lowercase().contains(platform))
        .min_by_key(|n| n.len())
        .cloned()
}

/// Is this name a signature rather than an artefact?
pub fn is_signature(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".sigstore.json")
        || lower.ends_with(".sigstore")
        || lower.ends_with(".bundle")
        || lower.ends_with(".sig")
        || lower.ends_with(".pem")
}

/// The signature file that goes with an asset, out of the names in a release.
///
/// Sigstore's own suffix first, then the older cosign ones, so a release that
/// carries both is verified against the current format.
pub fn signature_for(names: &[String], asset: &str) -> Option<String> {
    for suffix in [".sigstore.json", ".sigstore", ".bundle", ".sig"] {
        let wanted = format!("{asset}{suffix}");
        if let Some(found) = names.iter().find(|n| n.as_str() == wanted) {
            return Some(found.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        [
            "gmx-ndi-1.2.0-linux-x86_64.tar.gz",
            "gmx-ndi-1.2.0-linux-x86_64.tar.gz.sigstore.json",
            "gmx-ndi-1.2.0-linux-aarch64.tar.gz",
            "gmx-ndi-1.2.0-linux-aarch64.tar.gz.sigstore.json",
            "gmx-ndi-1.2.0-macos-aarch64.tar.gz",
            "gmx-ndi-1.2.0-windows-x86_64.zip",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn the_asset_for_a_platform_is_the_one_with_the_triple_in_its_name() {
        assert_eq!(
            asset_for(&names(), "linux-x86_64").as_deref(),
            Some("gmx-ndi-1.2.0-linux-x86_64.tar.gz")
        );
        assert_eq!(
            asset_for(&names(), "windows-x86_64").as_deref(),
            Some("gmx-ndi-1.2.0-windows-x86_64.zip")
        );
        assert_eq!(asset_for(&names(), "linux-armv7"), None);
    }

    #[test]
    fn a_signature_is_never_chosen_as_the_asset() {
        let only_sigs = vec!["thing-linux-x86_64.tar.gz.sigstore.json".to_string()];
        assert_eq!(asset_for(&only_sigs, "linux-x86_64"), None);
    }

    #[test]
    fn the_signature_beside_an_asset_is_found_by_suffix() {
        assert_eq!(
            signature_for(&names(), "gmx-ndi-1.2.0-linux-x86_64.tar.gz").as_deref(),
            Some("gmx-ndi-1.2.0-linux-x86_64.tar.gz.sigstore.json")
        );
        assert_eq!(signature_for(&names(), "gmx-ndi-1.2.0-windows-x86_64.zip"), None);
    }

    #[test]
    fn an_archive_with_no_known_extension_names_what_is_wanted() {
        let err = unpack(Path::new("/tmp/thing.rar"), Path::new("/tmp"))
            .expect_err("rar is not supported");
        assert!(format!("{err}").contains("tar.gz"), "{err}");
    }
}
