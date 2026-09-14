//! Finding the NDI runtime without linking against it.
//!
//! NDI's runtime is not ours to ship. Its licence forbids redistribution and
//! the NDI trademark belongs to Vizrt, so the plugin must work on a machine
//! where the runtime is absent: it says where to get it and stops, rather than
//! failing to load or taking the mixer down with it. That is what `dlopen`
//! buys, and it is why this crate links nothing NDI at build time.
//!
//! This module only ever opens the library and asks whether one known symbol is
//! there. The media path is GStreamer's `ndisrc` and `ndisink`, which do their
//! own loading; this check exists so the plugin can refuse with a sentence the
//! reader can act on instead of "Failed loading NDI SDK".
//!
//! DistroAV (formerly obs-ndi) documented the licence and packaging friction
//! this design avoids. The plugin's README carries the attribution.

use std::path::PathBuf;

/// The one symbol whose presence proves this is the NDI runtime.
const SYMBOL: &[u8] = b"NDIlib_initialize\0";

/// Where the runtime was found, for a log line and for `stats`.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub path: String,
}

/// The environment variables the NDI SDK itself defines, newest first.
///
/// An installer sets these, so honouring them is how a runtime in a place
/// nobody guessed is still found.
const ENV_DIRS: &[&str] = &[
    "NDI_RUNTIME_DIR_V6",
    "NDI_RUNTIME_DIR_V5",
    "NDI_RUNTIME_DIR_V4",
];

/// The library file names, newest soname first.
fn file_names() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["Processing.NDI.Lib.x64.dll", "Processing.NDI.Lib.x86.dll"]
    } else if cfg!(target_os = "macos") {
        &["libndi.dylib", "libndi.4.dylib"]
    } else {
        &["libndi.so.6", "libndi.so.5", "libndi.so.4", "libndi.so"]
    }
}

/// The directories to look in when no environment variable says where.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for key in ENV_DIRS {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                dirs.push(PathBuf::from(value));
            }
        }
    }
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/usr/local/lib"));
        dirs.push(PathBuf::from("/opt/homebrew/lib"));
        dirs.push(PathBuf::from("/Library/NDI SDK for Apple/lib/macOS"));
    } else if cfg!(target_os = "windows") {
        if let Ok(files) = std::env::var("ProgramFiles") {
            let root = PathBuf::from(files).join("NDI");
            dirs.push(root.join("NDI 6 Runtime").join("v6"));
            dirs.push(root.join("NDI 5 Runtime").join("v5"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/lib"));
        dirs.push(PathBuf::from("/usr/local/lib"));
        dirs.push(PathBuf::from("/usr/lib/x86_64-linux-gnu"));
        dirs.push(PathBuf::from("/usr/lib/aarch64-linux-gnu"));
    }
    dirs
}

/// Every path worth trying, in order. The bare file name comes first so the
/// ordinary loader search (`LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH`, `PATH`) gets
/// its say before any guess of ours.
pub fn candidates() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in file_names() {
        out.push((*name).to_string());
    }
    for dir in search_dirs() {
        for name in file_names() {
            out.push(dir.join(name).to_string_lossy().to_string());
        }
    }
    out
}

/// Open the runtime, or say why not.
///
/// The library is closed again straight away: nothing here calls into it, and
/// holding it open would mean this process could not be told to let go of a
/// runtime being upgraded underneath it.
pub fn find() -> Result<Found, String> {
    for candidate in candidates() {
        // Opening a library runs its initialisers, which is why this is behind
        // an explicit call rather than done at start up.
        let opened = unsafe { libloading::Library::new(&candidate) };
        let Ok(library) = opened else { continue };
        let symbol = unsafe { library.get::<unsafe extern "C" fn() -> bool>(SYMBOL) };
        if symbol.is_ok() {
            return Ok(Found { path: candidate });
        }
    }
    Err(missing())
}

/// What to tell somebody who has no runtime.
pub fn missing() -> String {
    format!(
        "the NDI runtime is not on this machine, so NDI sources and outputs cannot work. \
         It is a free download from https://ndi.video/for-developers/ndi-sdk/ (the \
         runtime alone is enough; the SDK is not needed). GodwinMix does not ship it: \
         its licence does not allow redistribution. Install it, or set one of {} to the \
         directory holding {}, and start the plugin again.",
        ENV_DIRS.join(", "),
        file_names().join(" or ")
    )
}

/// Whether the GStreamer elements are here, which is a separate question from
/// whether the runtime is.
pub fn elements_present() -> bool {
    gmx_netkit::elements::exists("ndisrc") && gmx_netkit::elements::exists("ndisrcdemux")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_candidates_start_with_the_bare_name_so_the_loader_search_wins() {
        let list = candidates();
        assert!(!list.is_empty());
        assert!(!list[0].contains(std::path::MAIN_SEPARATOR));
        assert_eq!(list[0], file_names()[0]);
    }

    #[test]
    fn this_platforms_file_names_are_the_ones_ndi_actually_ships() {
        let names = file_names();
        if cfg!(target_os = "windows") {
            assert!(names[0].ends_with(".dll"));
        } else if cfg!(target_os = "macos") {
            assert!(names[0].ends_with(".dylib"));
        } else {
            assert!(names[0].starts_with("libndi.so"));
        }
    }

    #[test]
    fn an_environment_variable_is_searched_before_the_guesses() {
        // Safety: the test process sets and clears one variable it owns, and
        // the read is in the same thread.
        std::env::set_var("NDI_RUNTIME_DIR_V6", "/somewhere/nobody/guessed");
        let dirs = search_dirs();
        std::env::remove_var("NDI_RUNTIME_DIR_V6");
        assert_eq!(dirs[0], PathBuf::from("/somewhere/nobody/guessed"));
    }

    #[test]
    fn the_message_for_a_missing_runtime_names_the_download_and_the_reason() {
        let message = missing();
        assert!(message.contains("ndi.video"), "{message}");
        assert!(message.contains("redistribution"), "{message}");
        assert!(message.contains("NDI_RUNTIME_DIR_V6"), "{message}");
    }

    #[test]
    fn finding_it_or_not_both_answer_rather_than_panicking() {
        match find() {
            Ok(found) => assert!(!found.path.is_empty()),
            Err(why) => assert!(why.contains("ndi.video"), "{why}"),
        }
    }
}
