//! The device plugins that travel inside the app.
//!
//! A camera, a screen and a microphone are what somebody expects to find when
//! they open a video mixer. All three are plugins rather than built in kinds,
//! so an app that carried none of them would send its operator to a terminal
//! for `gmx plugin add camera`, and a terminal is the thing this app exists to
//! stand between them and.
//!
//! `dev/bundle-plugins.sh` stages them into the bundle as
//! `resources/plugins/<platform>/<name>/<version>/`. This module copies that
//! into `<app data>/plugins` before the mixer starts, and `sidecar` points the
//! mixer at the copy with `GODWINMIX_PLUGINS_DIR`.
//!
//! Copied, rather than the mixer being pointed at the bundle where it lies,
//! for two reasons. An installed app's own directory is read only on all three
//! platforms, and `plugin.add` from the window has to be able to put a fourth
//! plugin beside these three; a read only directory would make the button in
//! the UI a lie. And the copy is what makes an upgrade work: each seeded
//! version carries a stamp of the bundle it came from, so a newer app replaces
//! its own copy and leaves anything the operator installed themselves alone.
//!
//! A build with no plugins staged in it changes nothing. The mixer then reads
//! the plugins directory it would have read anyway, which is how a developer
//! build keeps working and how somebody who has already installed these three
//! by hand keeps the ones they installed.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// Written inside each seeded `<name>/<version>` directory, holding the stamp
/// of the bundled copy it was made from. Its presence is also what marks a
/// directory as this app's to replace: a plugin the operator installed has no
/// such file and is never touched.
const STAMP: &str = ".gmx-bundled";

/// The plugins directory the mixer should be started against, with the
/// bundled plugins in it.
///
/// `None` when this build carries none, which leaves the mixer's own default
/// in force.
pub fn ensure(app: &AppHandle) -> Option<PathBuf> {
    let bundled = bundled(app)?;
    let dest = crate::settings::plugins_dir(app)
        .map_err(|e| eprintln!("[desktop] no plugins directory: {e}"))
        .ok()?;
    if let Err(e) = seed(&bundled, &dest) {
        // Not fatal. The mixer starts either way, and a mixer with no camera
        // plugin is a great deal better than a mixer that would not start.
        eprintln!("[desktop] could not put the bundled plugins in place: {e}");
    }
    Some(dest)
}

/// `resources/plugins/<platform>` inside the installed app, when there is
/// really something in it.
///
/// The per platform directory first and the flat one after it, the same way
/// the bundled GStreamer is found, so a tree assembled by hand also works. An
/// empty directory is not a set of plugins: the repository keeps
/// `tauri-app/plugins/` with only a `.gitignore` in it, and that must not be
/// read as "this app carries plugins".
pub fn bundled(app: &AppHandle) -> Option<PathBuf> {
    let root = app.path().resource_dir().ok()?.join("plugins");
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    [root.join(os), root].into_iter().find(|dir| !versions(dir).is_empty())
}

/// Every `<name>/<version>` under a directory that has a manifest in it.
fn versions(root: &Path) -> Vec<(String, String, PathBuf)> {
    let mut found = Vec::new();
    let Ok(names) = fs::read_dir(root) else { return found };
    for name in names.flatten().filter(|e| e.path().is_dir()) {
        let Ok(entries) = fs::read_dir(name.path()) else { continue };
        for version in entries.flatten().filter(|e| e.path().join("gmx-plugin.toml").is_file()) {
            found.push((
                name.file_name().to_string_lossy().into_owned(),
                version.file_name().to_string_lossy().into_owned(),
                version.path(),
            ));
        }
    }
    found
}

/// Put the bundled plugins in the operator's plugins directory, and take this
/// app's older copies away again.
fn seed(bundled: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for (name, version, from) in versions(bundled) {
        let stamp = stamp_of(&from);
        let into = dest.join(&name).join(&version);
        match fs::read_to_string(into.join(STAMP)) {
            // The copy already there is the one in this bundle.
            Ok(there) if there.trim() == stamp.trim() => continue,
            // This app's copy from an older bundle, or from a rebuild of the
            // same version, which is what a developer does all day.
            Ok(_) => fs::remove_dir_all(&into)?,
            // Something is there that this app did not put there: somebody's
            // own build or their own install of the same version. Theirs
            // wins, always. Deleting a plugin an operator installed, to
            // replace it with our copy of it, is not a thing an app may do.
            Err(_) if into.exists() => continue,
            Err(_) => {}
        }
        copy_tree(&from, &into)?;
        fs::write(into.join(STAMP), &stamp)?;
        retire_older(&dest.join(&name), &version)?;
        eprintln!("[desktop] put {name} {version} in {}", dest.display());
    }
    Ok(())
}

/// What is in a bundled plugin directory, in one line: enough to notice a new
/// version, and enough to notice the same version rebuilt.
///
/// Not a hash. This runs at every launch on a few megabytes of binary, and the
/// question it answers is "is this the copy I made last time", which sizes and
/// modification times answer at the cost of a directory walk.
fn stamp_of(root: &Path) -> String {
    let (mut files, mut bytes, mut newest) = (0u64, 0u64, 0u64);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            files += 1;
            if let Ok(meta) = entry.metadata() {
                bytes += meta.len();
                let at = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                newest = newest.max(at);
            }
        }
    }
    format!("{files} files, {bytes} bytes, newest {newest}")
}

/// Take away this app's copies of other versions of the same plugin.
///
/// The core reads every version directory it finds and the last one read wins,
/// in whatever order the file system hands them over. Two versions of the
/// camera would therefore be a coin toss at every start. Only directories this
/// app seeded are removed; one the operator put there is left where it is, and
/// then it is theirs that wins, which is the right way round.
fn retire_older(name_dir: &Path, keep: &str) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(name_dir) else { return Ok(()) };
    for entry in entries.flatten() {
        if entry.file_name() == keep || !entry.path().join(STAMP).is_file() {
            continue;
        }
        fs::remove_dir_all(entry.path())?;
    }
    Ok(())
}

/// A directory, recursively, with the permission bits kept: the plugin's
/// binary has to arrive executable or the core cannot run it.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn fixture(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("gmx-desktop-test-{tag}"));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn a_directory_with_no_manifest_in_it_holds_no_plugins() {
        let root = fixture("plugins-empty");
        fs::create_dir_all(root.join("camera/0.1.0/bin")).unwrap();
        assert!(versions(&root).is_empty(), "a version directory with no manifest is not a plugin");
        write(&root.join("camera/0.1.0/gmx-plugin.toml"), "[plugin]\n");
        assert_eq!(versions(&root).len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn seeding_copies_once_and_then_leaves_it_alone() {
        let root = fixture("plugins-seed");
        let from = root.join("bundle");
        let dest = root.join("data");
        write(&from.join("camera/0.1.0/gmx-plugin.toml"), "[plugin]\nname = \"camera\"\n");
        write(&from.join("camera/0.1.0/bin/gmx-camera"), "binary");

        seed(&from, &dest).unwrap();
        let landed = dest.join("camera/0.1.0");
        assert!(landed.join("bin/gmx-camera").is_file(), "the binary travels with the manifest");
        assert!(landed.join(STAMP).is_file());

        // A second launch must not copy anything again, which is what the
        // stamp is for. Proven by editing the copy and watching it survive.
        write(&landed.join("bin/gmx-camera"), "edited");
        seed(&from, &dest).unwrap();
        assert_eq!(fs::read_to_string(landed.join("bin/gmx-camera")).unwrap(), "edited");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_newer_bundle_replaces_this_apps_copy_and_retires_the_old_version() {
        let root = fixture("plugins-upgrade");
        let from = root.join("bundle");
        let dest = root.join("data");
        write(&from.join("camera/0.1.0/gmx-plugin.toml"), "[plugin]\n");
        seed(&from, &dest).unwrap();
        assert!(dest.join("camera/0.1.0").is_dir());

        fs::remove_dir_all(from.join("camera/0.1.0")).unwrap();
        write(&from.join("camera/0.2.0/gmx-plugin.toml"), "[plugin]\n");
        seed(&from, &dest).unwrap();
        assert!(dest.join("camera/0.2.0").is_dir(), "the new version is in place");
        assert!(
            !dest.join("camera/0.1.0").exists(),
            "two versions of one plugin is a coin toss at every start"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_plugin_the_operator_installed_is_never_overwritten() {
        let root = fixture("plugins-theirs");
        let from = root.join("bundle");
        let dest = root.join("data");
        write(&from.join("camera/0.1.0/gmx-plugin.toml"), "[plugin]\nname = \"bundled\"\n");
        // Theirs: same name, same version, no stamp, because they built it.
        write(&dest.join("camera/0.1.0/gmx-plugin.toml"), "[plugin]\nname = \"theirs\"\n");

        seed(&from, &dest).unwrap();
        let text = fs::read_to_string(dest.join("camera/0.1.0/gmx-plugin.toml")).unwrap();
        assert!(text.contains("theirs"), "the app replaced a plugin it did not install");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_stamp_notices_a_rebuild_of_the_same_version() {
        let root = fixture("plugins-stamp");
        let dir = root.join("camera/0.1.0");
        write(&dir.join("gmx-plugin.toml"), "[plugin]\n");
        let first = stamp_of(&dir);
        write(&dir.join("bin/gmx-camera"), "a longer file than nothing at all");
        assert_ne!(first, stamp_of(&dir), "a file that arrived has to change the stamp");
        let _ = fs::remove_dir_all(&root);
    }
}
