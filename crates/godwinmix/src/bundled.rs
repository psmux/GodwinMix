//! Run the installed `godwinmix.exe` from anywhere on Windows.
//!
//! The installer puts a trimmed GStreamer beside the mixer, in
//! `gstreamer\windows`, and the desktop app hands the mixer that folder in
//! PATH and four GStreamer variables when it starts it. Started any other way,
//! by an AI agent running `godwinmix mcp`, by a person typing `godwinmix tool
//! list`, by a service manager, the mixer had none of that and Windows refused
//! to start it at all: its GStreamer DLLs were nowhere it looks
//! (0xC0000135).
//!
//! So the GStreamer DLLs are delay loaded (see `build.rs`): nothing of them is
//! needed until the first call into GStreamer, and this runs first, as the
//! first line of `main`, and points the process at the bundled copy. A
//! variable already set, by the desktop app or by a person, is left alone.

/// Point this process at the GStreamer beside it, when there is one.
#[cfg(windows)]
pub fn prepare() {
    use std::path::{Path, PathBuf};
    let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) else { return };
    let root = exe_dir.join("gstreamer").join("windows");
    let bin = root.join("bin");
    if !bin.join("gstreamer-1.0-0.dll").is_file() {
        // A checkout or a machine with GStreamer installed: its own PATH
        // already finds it.
        return;
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    if !std::env::split_paths(&path).any(|p| p == bin) {
        let joined = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(&path)));
        if let Ok(joined) = joined {
            // Before any thread exists: this is the first line of main.
            std::env::set_var("PATH", joined);
        }
    }
    if std::env::var_os("GST_PLUGIN_SYSTEM_PATH").is_some() {
        return;
    }
    let plugins = root.join("lib").join("gstreamer-1.0");
    std::env::set_var("GST_PLUGIN_SYSTEM_PATH", &plugins);
    std::env::set_var("GST_PLUGIN_PATH", &plugins);
    let scanner = root.join("libexec").join("gstreamer-1.0").join("gst-plugin-scanner.exe");
    if scanner.is_file() {
        std::env::set_var("GST_PLUGIN_SCANNER", scanner);
    }
    if std::env::var_os("GST_REGISTRY").is_none() {
        // Somewhere writable: the install folder is not.
        let dir: Option<PathBuf> = crate::address::desktop_data_dir()
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("GodwinMix")));
        if let Some(dir) = dir {
            let _ = std::fs::create_dir_all(&dir);
            std::env::set_var("GST_REGISTRY", dir.join("gstreamer-registry.bin"));
        }
    }
}

#[cfg(not(windows))]
pub fn prepare() {}
