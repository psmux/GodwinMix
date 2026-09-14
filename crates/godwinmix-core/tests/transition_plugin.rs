//! The example transition plugin, against the harness's transition checks.
//!
//! `plugins/wipe` is the worked example of the fourth plugin kind, and this is
//! what says it works: the manifest is valid, the process starts and shakes
//! hands, and `render` answers the contract at the start, the middle and the
//! end of a crossing. Nothing here knows what a wipe looks like; what is
//! checked is what any transition plugin has to do, so a dissolve or a clock
//! wipe written by somebody else passes the same test.

#![cfg(unix)]

use godwinmix_core::plugin::harness;
use std::path::PathBuf;

/// The plugin's directory in the checkout.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/wipe")
}

/// Put the built binary where the manifest says it is.
///
/// A plugin under `plugins/` is a workspace member, so cargo builds it into
/// the workspace's own target directory, while its manifest names a path
/// inside the plugin (`bin/gmx-wipe`) because that is what an installed copy
/// looks like. The copy is one line here and one line in the plugin's README.
fn built() -> Option<PathBuf> {
    let name = "gmx-wipe";
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let built = ["release", "debug"]
        .iter()
        .map(|profile| workspace.join(profile).join(name))
        .find(|p| p.is_file())?;
    let bin = root().join("bin");
    std::fs::create_dir_all(&bin).ok()?;
    let at = bin.join(name);
    std::fs::copy(&built, &at).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o755)).ok()?;
    }
    Some(at)
}

#[test]
fn the_wipe_plugin_passes_the_transition_checks() {
    let manifest = harness::check_manifest(&root());
    assert!(manifest.passed, "{}: {}", manifest.name, manifest.detail);

    let Some(_binary) = built() else {
        // `cargo test -p godwinmix-core` on its own does not build the plugin.
        // Saying so beats a failure that looks like the plugin is broken.
        println!(
            "gmx-wipe is not built, so the transition checks were skipped. Build it with \
             `cargo build -p gmx-wipe` and run this again."
        );
        return;
    };

    let spawn = harness::check_spawn(&root(), "wipe");
    assert!(spawn.passed, "{}: {}", spawn.name, spawn.detail);
    let configure = harness::check_configure(&root(), "wipe");
    assert!(configure.passed, "{}: {}", configure.name, configure.detail);

    let transition = harness::check_transition(&root(), "wipe");
    println!("{}: {}", transition.name, transition.detail);
    assert!(transition.passed, "{}: {}", transition.name, transition.detail);
}
