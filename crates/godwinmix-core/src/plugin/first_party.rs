//! The plugins that ship with this mixer, found by name with no network.
//!
//! `plugin.add {source: "camera"}` used to mean a marketplace lookup and
//! nothing else, so a mixer with no marketplace configured could not install
//! its own camera, screen or ingest plugin from the picker. A bare name is now
//! looked for here first, in these places and nowhere else:
//!
//! ```text
//!   <exe dir>/../share/godwinmix/plugins/<name>   an install under a prefix
//!   <exe dir>/plugins/<name>                      everything in one folder
//!   <checkout>/plugins/<name>                     a build run from a source checkout
//! ```
//!
//! A checkout is the first folder above the executable, up to four levels,
//! with both a `Cargo.toml` and a `plugins` folder in it: `cargo run` puts the
//! binary in `target/debug` or `target/<triple>/release` under it. A plugin
//! there is taken only when it can run: its binary for this platform is built,
//! or its manifest has a `[build]` section the install runs, or it declares no
//! binary at all (a script). Whatever is found is installed as a folder is,
//! with the same "custom, unreviewed" trust record.

use godwinmix_host::launch;
use godwinmix_protocol::plugin::manifest::Manifest;
use std::path::{Path, PathBuf};

/// Where a plugin of this name would be, in the order they are looked at.
pub fn places(name: &str) -> Vec<PathBuf> {
    let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf))
    else {
        return Vec::new();
    };
    places_from(&exe_dir, name)
}

/// The first place that holds a usable plugin called `name`.
pub fn find(name: &str) -> Option<PathBuf> {
    if !is_plain_name(name) {
        return None;
    }
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    find_from(&exe_dir, name)
}

fn find_from(exe_dir: &Path, name: &str) -> Option<PathBuf> {
    let checkout = checkout_of(exe_dir);
    places_from(exe_dir, name).into_iter().find(|dir| {
        let in_checkout = checkout.as_ref().is_some_and(|c| dir.starts_with(c));
        usable(dir, name, in_checkout)
    })
}

fn places_from(exe_dir: &Path, name: &str) -> Vec<PathBuf> {
    let mut out = vec![
        exe_dir.join("..").join("share").join("godwinmix").join("plugins").join(name),
        exe_dir.join("plugins").join(name),
    ];
    out.extend(checkout_of(exe_dir).map(|c| c.join("plugins").join(name)));
    out
}

/// The source checkout this executable was built in, if it was.
fn checkout_of(exe_dir: &Path) -> Option<PathBuf> {
    exe_dir
        .ancestors()
        .skip(1)
        .take(4)
        .find(|d| d.join("Cargo.toml").is_file() && d.join("plugins").is_dir())
        .map(Path::to_path_buf)
}

/// A plugin folder with a manifest that names it `name`, and, in a checkout,
/// one that can run as it stands or be built by the install.
fn usable(dir: &Path, name: &str, in_checkout: bool) -> bool {
    // Parsed and not validated: validating asks that the binary exist, and in
    // a checkout that has not built it yet the `[build]` section is the
    // answer. The install validates in full once it has run.
    let text = std::fs::read_to_string(dir.join("gmx-plugin.toml")).unwrap_or_default();
    let Ok(manifest) = Manifest::parse(&text) else { return false };
    if manifest.plugin.name != name {
        return false;
    }
    if !in_checkout {
        return true;
    }
    match manifest.run.as_ref().and_then(|r| r.bin.get(launch::this_platform())) {
        Some(bin) => dir.join(bin).is_file() || manifest.build.is_some(),
        None => true,
    }
}

/// Letters, digits, `-` and `_`: a name, never a path.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
#[path = "first_party_tests.rs"]
mod tests;
