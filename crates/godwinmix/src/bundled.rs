//! Run the installed mixer from anywhere, not only from the desktop app.
//!
//! The installers put a trimmed GStreamer beside the mixer, and the desktop
//! app hands the mixer that folder in four GStreamer variables (and PATH on
//! Windows) when it starts it. Started any other way, by an AI agent running
//! `godwinmix mcp`, by a person typing `godwinmix tool list`, by a service
//! manager, the mixer had none of that. On Windows it could not start at all:
//! its GStreamer DLLs were nowhere Windows looks (0xC0000135).
//!
//! So this runs first, as the first line of `main`, and points the process at
//! the bundled copy. On Windows the GStreamer DLLs are delay loaded (see
//! `build.rs`), so nothing of them is needed before this has set PATH. On
//! macOS the release rewrites the binary's library paths to the ones inside
//! the app (see the release workflow), and this sets where the plugins are. A
//! variable already set, by the desktop app or by a person, is left alone.
//! Linux packages use the system's GStreamer and need nothing here.

use std::path::{Path, PathBuf};

/// Point this process at the GStreamer beside it, when there is one.
pub fn prepare() {
    let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) else { return };
    let Some(root) = root(&exe_dir) else { return };
    #[cfg(windows)]
    add_to_path(&root.join("bin"));
    if std::env::var_os("GST_PLUGIN_SYSTEM_PATH").is_some() {
        return;
    }
    let plugins = root.join("lib").join("gstreamer-1.0");
    // Before any thread exists: this is the first line of main.
    std::env::set_var("GST_PLUGIN_SYSTEM_PATH", &plugins);
    std::env::set_var("GST_PLUGIN_PATH", &plugins);
    let scanner = root.join("libexec").join("gstreamer-1.0").join(if cfg!(windows) { "gst-plugin-scanner.exe" } else { "gst-plugin-scanner" });
    if scanner.is_file() {
        std::env::set_var("GST_PLUGIN_SCANNER", scanner);
    }
    if std::env::var_os("GST_REGISTRY").is_none() {
        // Somewhere writable: an installed app's folder is not.
        if let Some(dir) = crate::address::desktop_data_dir() {
            let _ = std::fs::create_dir_all(&dir);
            std::env::set_var("GST_REGISTRY", dir.join("gstreamer-registry.bin"));
        }
    }
}

/// The bundled runtime, where the installer for this platform puts it.
fn root(exe_dir: &Path) -> Option<PathBuf> {
    let root = if cfg!(windows) {
        exe_dir.join("gstreamer").join("windows")
    } else if cfg!(target_os = "macos") {
        // GodwinMix.app/Contents/MacOS/godwinmix, Contents/Resources/gstreamer.
        exe_dir.parent()?.join("Resources").join("gstreamer").join("macos")
    } else {
        return None;
    };
    root.join("lib").join("gstreamer-1.0").is_dir().then_some(root)
}

#[cfg(windows)]
fn add_to_path(bin: &Path) {
    let path = std::env::var_os("PATH").unwrap_or_default();
    if std::env::split_paths(&path).any(|p| p == bin) {
        return;
    }
    if let Ok(joined) = std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&path))) {
        std::env::set_var("PATH", joined);
    }
}
