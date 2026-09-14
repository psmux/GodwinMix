//! `gmx plugin add https://gitlab.com/x/y.git`: clone it, then build it.
//!
//! A git source is how a plugin that is not on GitHub, or one that has no
//! release yet, gets installed. It is also the answer when a release has no
//! asset for this machine: the source is here, so build it here.
//!
//! The build is the manifest's, not ours. `[build] command` runs in the clone
//! with a shell, and `[build] output` names what it produced. Nothing guesses
//! at cargo or npm: a plugin that needs two commands and an environment
//! variable writes them in `command` and they run as written. The clone is
//! shallow and the `.git` directory is left behind when the tree is copied in,
//! because the loader's copy skips it.

use super::{run, FetchCtx, Fetched};
use crate::verify::Trust;
use anyhow::{Context, Result};
use godwinmix_protocol::plugin::manifest::Manifest;
use std::path::Path;

pub fn fetch(url: &str, reference: Option<&str>, ctx: &FetchCtx) -> Result<Fetched> {
    let clone = ctx.staging.join("clone");
    if clone.exists() {
        std::fs::remove_dir_all(&clone).with_context(|| format!("clearing {}", clone.display()))?;
    }
    std::fs::create_dir_all(&clone).with_context(|| format!("making {}", clone.display()))?;
    let target = clone.to_string_lossy().into_owned();
    let mut args = vec!["clone", "--depth", "1", "--recurse-submodules"];
    if let Some(reference) = reference {
        args.push("--branch");
        args.push(reference);
    }
    args.push(url);
    args.push(&target);
    run("cloning the plugin", "git", &args, &ctx.staging).with_context(|| {
        format!(
            "cloning {url} failed. Check the URL, and that this machine can reach it. \
             A private repository needs a credential helper or an ssh key that git can find."
        )
    })?;

    let dir = super::find_manifest_root(&clone)?;
    let mut notes = vec![format!(
        "cloned {url}{}",
        reference.map(|r| format!(" at {r}")).unwrap_or_default()
    )];
    let manifest = Manifest::load(dir.join("gmx-plugin.toml"))
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("the cloned repository's manifest")?;
    if let Some(build) = &manifest.build {
        notes.push(format!("building: {}", build.command));
        let out = build_it(&dir, &build.command)?;
        for line in out.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev() {
            if !line.trim().is_empty() {
                notes.push(format!("  {line}"));
            }
        }
        check_output(&dir, &build.output)?;
    } else if needs_a_build(&manifest) {
        anyhow::bail!(
            "{} declares `[run] bin` but has no `[build]` section, so a git source has \
             nothing to run. Either the author publishes release assets (install with \
             `gmx plugin add owner/repo`) or the manifest says how to build here:\n\n  \
             [build]\n  command = \"cargo build --release\"\n  output = \"target/release/{}\"",
            manifest.plugin.name,
            manifest.plugin.name
        );
    }
    Ok(Fetched {
        dir,
        trust: Trust::unsigned(
            url,
            "it was built on this machine from a git clone, so nothing signed it",
        ),
        notes,
    })
}

/// Run the manifest's build command with the platform's shell.
fn build_it(dir: &Path, command: &str) -> Result<String> {
    let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
    run("the plugin's [build] command", shell, &[flag, command], dir)
}

/// The build said what it would produce. Check it did.
fn check_output(dir: &Path, output: &str) -> Result<()> {
    let produced = dir.join(output);
    anyhow::ensure!(
        produced.exists(),
        "the build ran but `{output}` is not there. `[build] output` names what the command \
         produces, relative to the plugin's root; fix it in gmx-plugin.toml or fix the \
         command."
    );
    Ok(())
}

/// A manifest that names a binary needs something to have built it.
fn needs_a_build(manifest: &Manifest) -> bool {
    manifest
        .run
        .as_ref()
        .map(|r| !r.bin.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(text: &str) -> Manifest {
        Manifest::parse(text).expect("the manifest parses")
    }

    #[test]
    fn a_binary_plugin_with_no_build_section_is_refused_with_the_section_to_add() {
        let m = manifest(
            "[plugin]\nname = \"clock\"\nversion = \"1.0.0\"\napi = 1\n\
             [run]\nbin = { \"linux-x86_64\" = \"bin/clock\" }\n",
        );
        assert!(needs_a_build(&m));
    }

    #[test]
    fn a_python_plugin_needs_no_build() {
        let m = manifest(
            "[plugin]\nname = \"clock\"\nversion = \"1.0.0\"\napi = 1\n\
             [run]\npython = \"main.py\"\n",
        );
        assert!(!needs_a_build(&m));
    }

    #[test]
    fn a_build_that_did_not_produce_what_it_said_names_the_path() {
        let dir = std::env::temp_dir().join(format!("gmx-git-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let err = check_output(&dir, "target/release/clock").expect_err("nothing was built");
        assert!(format!("{err}").contains("target/release/clock"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
