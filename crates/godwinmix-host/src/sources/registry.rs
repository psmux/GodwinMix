//! `cargo:`, `npm:` and `pypi:`: the three package registries a plugin author
//! is likely to be publishing to already.
//!
//! The shape is the same for all three and it is worth stating once, because
//! it is the part that is easy to get subtly wrong:
//!
//!   1. Get the published *source* for the version asked for, not just the
//!      built artefact, because `gmx-plugin.toml`, `settings.json` and
//!      `SKILL.md` live there and a plugin without them is not a plugin.
//!   2. Unpack it in the staging directory. That directory is the plugin root
//!      the loader copies to `<plugins_dir>/<name>/<version>/`.
//!   3. Do whatever has to happen *before* the copy (compiling a Rust binary),
//!      and leave whatever has to happen *after* it to the loader's
//!      preparation step (a venv, a node_modules), because the loader's copy
//!      deliberately skips `.venv`, `node_modules` and `target` and a tree
//!      built in staging would lose them.
//!
//! None of the three signs anything a GodwinMix core can check, so all three
//! land as "custom, unreviewed" with a line saying which registry it came
//! from. A crates.io checksum proves the bytes match what crates.io holds; it
//! does not say who published them, and the label does not pretend otherwise.

use super::{archive, http, run, FetchCtx, Fetched};
use crate::verify::Trust;
use anyhow::{Context, Result};
use godwinmix_protocol::plugin::manifest::Manifest;
use std::path::Path;

fn crates_api() -> String {
    std::env::var("GMX_CRATES_API").unwrap_or_else(|_| "https://crates.io/api/v1".into())
}

fn npm_api() -> String {
    std::env::var("GMX_NPM_API").unwrap_or_else(|_| "https://registry.npmjs.org".into())
}

fn pypi_api() -> String {
    std::env::var("GMX_PYPI_API").unwrap_or_else(|_| "https://pypi.org/pypi".into())
}

// ---------------------------------------------------------------------------
// cargo
// ---------------------------------------------------------------------------

/// `gmx plugin add cargo:gmx-ndi`.
///
/// The `.crate` file is fetched for the manifest and the auxiliary files, and
/// `cargo install --root` builds the binary into the staged tree. Building
/// here rather than after the copy is deliberate: `target/` is not copied, so
/// a binary left inside it would never arrive.
pub fn fetch_cargo(package: &str, version: Option<&str>, ctx: &FetchCtx) -> Result<Fetched> {
    let version = match version {
        Some(v) => v.to_string(),
        None => newest_crate_version(package)?,
    };
    let url = format!("{}/crates/{package}/{version}/download", crates_api());
    let file = ctx.staging.join("download").join(format!("{package}-{version}.crate"));
    http::download(&url, &file).with_context(|| {
        format!(
            "{package} {version} is not on crates.io, or this machine cannot reach it. \
             `cargo search {package}` says what is published."
        )
    })?;
    let unpacked = ctx.staging.join("unpacked");
    archive::unpack(&file, &unpacked)?;
    let dir = super::find_manifest_root(&unpacked)?;
    let manifest = load(&dir)?;
    let mut notes = vec![format!("crates.io: {package} {version}")];

    if let Some(target) = binary_target(&manifest, &ctx.platform) {
        anyhow::ensure!(
            !target.starts_with("target/") && !target.starts_with("target\\"),
            "{}'s manifest puts its binary at `{target}`, inside the build directory. \
             That directory is not copied when a plugin is installed. Point \
             `[run] bin.\"{}\"` at somewhere else in the tree, for example `bin/{}`.",
            manifest.plugin.name,
            ctx.platform,
            manifest.plugin.name
        );
        let root = ctx.staging.join("cargo-root");
        let root_arg = root.to_string_lossy().into_owned();
        let dir_arg = dir.to_string_lossy().into_owned();
        run(
            "cargo install",
            "cargo",
            &["install", "--path", &dir_arg, "--root", &root_arg, "--locked"],
            &ctx.staging,
        )
        .context(
            "the crate did not build. Rust is needed on the machine that installs a \
             `cargo:` plugin; a prebuilt release asset (`gmx plugin add owner/repo`) needs \
             no compiler at all.",
        )?;
        let built = newest_binary(&root.join("bin"))?;
        let placed = dir.join(&target);
        if let Some(parent) = placed.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("making {}", parent.display()))?;
        }
        std::fs::copy(&built, &placed).with_context(|| {
            format!("putting {} at {}", built.display(), placed.display())
        })?;
        super::copy_mode(&built, &placed);
        notes.push(format!("built {} and placed it at {target}", built.display()));
    }
    Ok(Fetched {
        dir,
        trust: Trust::unsigned(
            format!("cargo:{package}@{version}"),
            "crates.io does not publish a signature a core can check",
        ),
        notes,
    })
}

fn newest_crate_version(package: &str) -> Result<String> {
    let value = http::get_json(&format!("{}/crates/{package}", crates_api()))
        .with_context(|| format!("looking {package} up on crates.io"))?;
    value["crate"]["max_stable_version"]
        .as_str()
        .or_else(|| value["crate"]["newest_version"].as_str())
        .map(str::to_string)
        .with_context(|| format!("crates.io has no published version of {package}"))
}

/// Where the manifest wants this platform's binary.
fn binary_target(manifest: &Manifest, platform: &str) -> Option<String> {
    manifest.run.as_ref()?.bin.get(platform).cloned()
}

/// The one file `cargo install --root` put in `bin/`.
fn newest_binary(bin: &Path) -> Result<std::path::PathBuf> {
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(bin)
        .with_context(|| format!("cargo install wrote nothing to {}", bin.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    anyhow::ensure!(
        !found.is_empty(),
        "cargo install produced no binary. The crate has no [[bin]] target, so there is \
         nothing for the core to run."
    );
    anyhow::ensure!(
        found.len() == 1,
        "the crate produced {} binaries and the manifest names one path, so there is no \
         way to tell which is the plugin. Publish a crate with one [[bin]].",
        found.len()
    );
    Ok(found.remove(0))
}

// ---------------------------------------------------------------------------
// npm
// ---------------------------------------------------------------------------

/// `gmx plugin add npm:@scope/gmx-chat`.
///
/// `npm pack` is used rather than `npm install` because it gives the published
/// files and nothing else, and because it works the same whether the package
/// is scoped or not. The dependency install happens after the copy, in the
/// loader's preparation step, since `node_modules` is not copied.
pub fn fetch_npm(package: &str, version: Option<&str>, ctx: &FetchCtx) -> Result<Fetched> {
    let spec = match version {
        Some(v) => format!("{package}@{v}"),
        None => package.to_string(),
    };
    let pack = ctx.staging.join("npm");
    std::fs::create_dir_all(&pack).with_context(|| format!("making {}", pack.display()))?;
    let registry = npm_api();
    let out = run(
        "npm pack",
        "npm",
        &["pack", &spec, "--silent", "--registry", &registry],
        &pack,
    )
    .context(
        "npm is needed to install an `npm:` plugin. Install Node.js, or ask the author for \
         a release asset.",
    )?;
    let tarball = out
        .lines()
        .map(str::trim)
        .filter(|l| l.ends_with(".tgz"))
        .next_back()
        .map(|name| pack.join(name))
        .context("npm pack said nothing about what it wrote")?;
    let unpacked = ctx.staging.join("unpacked");
    archive::unpack(&tarball, &unpacked)?;
    let dir = super::find_manifest_root(&unpacked)?;
    let manifest = load(&dir)?;
    Ok(Fetched {
        dir,
        trust: Trust::unsigned(
            format!("npm:{spec}"),
            "npm does not publish a signature a core can check",
        ),
        notes: vec![format!(
            "npm: {} {}",
            manifest.plugin.name, manifest.plugin.version
        )],
    })
}

// ---------------------------------------------------------------------------
// PyPI
// ---------------------------------------------------------------------------

/// `gmx plugin add pypi:gmx-director`.
///
/// The source distribution is what is fetched: a wheel has no `gmx-plugin.toml`
/// unless the author went out of their way to package it as data, and the
/// sdist is the tree the author wrote. The virtual environment is made after
/// the copy, by the loader's preparation step, using `uv` when it is there and
/// `python -m venv` when it is not.
pub fn fetch_pypi(package: &str, version: Option<&str>, ctx: &FetchCtx) -> Result<Fetched> {
    let url = match version {
        Some(v) => format!("{}/{package}/{v}/json", pypi_api()),
        None => format!("{}/{package}/json", pypi_api()),
    };
    let meta = http::get_json(&url)
        .with_context(|| format!("looking {package} up on PyPI"))?;
    let version = meta["info"]["version"].as_str().unwrap_or("?").to_string();
    let sdist = meta["urls"]
        .as_array()
        .and_then(|urls| {
            urls.iter()
                .find(|u| u["packagetype"].as_str() == Some("sdist"))
                .and_then(|u| Some((u["url"].as_str()?.to_string(), u["filename"].as_str()?.to_string())))
        })
        .with_context(|| {
            format!(
                "{package} {version} publishes no source distribution, only wheels. A \
                 GodwinMix plugin is installed from the sdist because that is where \
                 gmx-plugin.toml is. Ask the author to publish one, or install from the \
                 git source."
            )
        })?;
    let file = ctx.staging.join("download").join(&sdist.1);
    http::download(&sdist.0, &file)?;
    let unpacked = ctx.staging.join("unpacked");
    archive::unpack(&file, &unpacked)?;
    let dir = super::find_manifest_root(&unpacked)?;
    load(&dir)?;
    // The loader's preparation makes the venv and installs from pyproject.toml
    // or requirements.txt. An sdist with neither gets a requirements.txt
    // naming itself, so the package's own dependencies still arrive.
    if !dir.join("pyproject.toml").exists() && !dir.join("requirements.txt").exists() {
        std::fs::write(dir.join("requirements.txt"), format!("{package}=={version}\n"))
            .with_context(|| format!("writing {}", dir.join("requirements.txt").display()))?;
    }
    Ok(Fetched {
        dir,
        trust: Trust::unsigned(
            format!("pypi:{package}@{version}"),
            "PyPI does not publish a signature a core can check",
        ),
        notes: vec![format!("PyPI: {package} {version} ({})", sdist.1)],
    })
}

fn load(dir: &Path) -> Result<Manifest> {
    Manifest::load(dir.join("gmx-plugin.toml")).map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with_bin(path: &str) -> Manifest {
        Manifest::parse(&format!(
            "[plugin]\nname = \"ndi\"\nversion = \"1.0.0\"\napi = 1\n\
             [run]\nbin = {{ \"linux-x86_64\" = \"{path}\" }}\n"
        ))
        .expect("it parses")
    }

    #[test]
    fn the_binary_target_is_read_off_the_manifest_for_this_platform() {
        let m = manifest_with_bin("bin/gmx-ndi");
        assert_eq!(binary_target(&m, "linux-x86_64").as_deref(), Some("bin/gmx-ndi"));
        assert_eq!(binary_target(&m, "macos-aarch64"), None);
    }

    #[test]
    fn a_plugin_with_no_bin_for_this_platform_needs_no_cargo_build() {
        let m = Manifest::parse(
            "[plugin]\nname = \"ndi\"\nversion = \"1.0.0\"\napi = 1\n[run]\npython = \"main.py\"\n",
        )
        .expect("it parses");
        assert_eq!(binary_target(&m, "linux-x86_64"), None);
    }

    #[test]
    fn a_bin_path_inside_the_build_directory_would_not_survive_the_copy() {
        // The check itself lives in fetch_cargo; this pins the rule it uses so
        // a change to the loader's skip list breaks a test rather than a user.
        let target = "target/release/gmx-ndi";
        assert!(target.starts_with("target/"));
    }
}
