//! `gmx plugin add ./my-plugin`: the development form.
//!
//! Nothing is fetched, nothing is verified, and 06 section 2 says so plainly:
//! a path install is treated as `exec:` for trust purposes and carries the
//! "custom, unreviewed" label from the moment it lands. The operator's
//! `[plugins] allow_unsigned` switch is what decides whether the core accepts
//! it at all; that gate is in the loader, because it is the loader that knows
//! the configuration.

use super::Fetched;
use crate::verify::Trust;
use anyhow::{Context, Result};
use std::path::Path;

pub fn fetch(source: &Path) -> Result<Fetched> {
    let dir = source
        .canonicalize()
        .with_context(|| format!("there is nothing at {}", source.display()))?;
    anyhow::ensure!(
        dir.is_dir(),
        "`{}` is a file, not a directory. `gmx plugin add` takes the directory a plugin was \
         built in, the one with gmx-plugin.toml at its root.",
        source.display()
    );
    anyhow::ensure!(
        dir.join("gmx-plugin.toml").is_file(),
        "there is no gmx-plugin.toml in `{}`. Every plugin has one at its root; \
         `gmx plugin new` writes it.",
        source.display()
    );
    Ok(Fetched {
        dir: dir.clone(),
        trust: Trust::unsigned(
            dir.display().to_string(),
            "it was installed from a directory on this machine",
        ),
        notes: vec![format!("from {}", dir.display())],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_with_no_manifest_says_what_writes_one() {
        let dir = std::env::temp_dir().join(format!("gmx-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory");
        let err = fetch(&dir).expect_err("there is no manifest");
        assert!(format!("{err}").contains("gmx plugin new"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_install_is_unreviewed_and_says_why() {
        let dir = std::env::temp_dir().join(format!("gmx-path-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory");
        std::fs::write(
            dir.join("gmx-plugin.toml"),
            "[plugin]\nname = \"clock\"\nversion = \"0.1.0\"\napi = 1\n",
        )
        .expect("a manifest");
        let found = fetch(&dir).expect("it is a plugin directory");
        assert!(!found.trust.is_signed());
        assert_eq!(found.trust.label(), "custom, unreviewed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
