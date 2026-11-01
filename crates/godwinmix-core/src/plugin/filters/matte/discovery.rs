//! Locate an installed runtime without starting inference or downloading files.

use super::runtime::LIBRARY;
use std::path::{Path, PathBuf};

pub(super) fn locate() -> std::result::Result<PathBuf, String> {
    let mut tried = Vec::new();
    for path in candidates() {
        if path.is_absolute() && !path.is_file() {
            tried.push(path.display().to_string());
            continue;
        }
        preload_beside(&path);
        // `ort` panics on a library it cannot use rather than answering, so
        // the panic is turned back into an answer here.
        let attempt = std::panic::catch_unwind(|| ort::init_from(&path).map(|b| b.with_name("godwinmix").commit()));
        match attempt {
            Ok(Ok(_)) => {
                tracing::info!(runtime = %path.display(), "loaded ONNX Runtime for the cutout");
                return Ok(path);
            }
            Ok(Err(e)) => tried.push(format!("{} ({e})", path.display())),
            Err(_) => tried.push(format!("{} (could not be used)", path.display())),
        }
    }
    Err(format!(
        "ONNX Runtime is not on this machine, so the person cutout cannot run. The desktop \
         installers carry it; on a server run dev/fetch-models.sh from a checkout, or install it from https://onnxruntime.ai (1.20 or \
         newer) or set ORT_DYLIB_PATH to its {LIBRARY}. Looked at: {}",
        tried.join(", ")
    ))
}

/// On Windows, load what the runtime needs from its own folder first. The
/// loader searches the application's folder and the system's, never the
/// folder of the library asking, so a DirectML.dll beside onnxruntime.dll in
/// a subfolder would not be found and the runtime would fall back to the CPU.
/// Loaded once and kept: a library the process already has is used by name.
#[cfg(windows)]
fn preload_beside(runtime: &Path) {
    let Some(dir) = runtime.parent().filter(|d| !d.as_os_str().is_empty()) else { return };
    for name in ["DirectML.dll", "onnxruntime_providers_shared.dll"] {
        let path = dir.join(name);
        if path.is_file() {
            // SAFETY: a library shipped beside the runtime, loaded for the
            // runtime to use; nothing is called through this handle.
            if let Ok(lib) = unsafe { libloading::Library::new(&path) } {
                std::mem::forget(lib);
            }
        }
    }
}

#[cfg(not(windows))]
fn preload_beside(_: &Path) {}

/// Where the runtime is looked for: the operator's choice, then beside the
/// mixer as the installers put it, then the system's own search.
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = std::env::var_os("ORT_DYLIB_PATH").filter(|p| !p.is_empty()) {
        out.push(PathBuf::from(p));
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        let prefix = exe_dir.parent().unwrap_or(&exe_dir).to_path_buf();
        // Beside the executable on Windows, so a DirectML.dll beside it is
        // found by the loader's own search when the runtime asks for it.
        out.push(exe_dir.join(LIBRARY));
        out.push(exe_dir.join("onnxruntime").join(LIBRARY));
        out.push(prefix.join("Frameworks").join(LIBRARY));
        out.push(prefix.join("Resources").join("onnxruntime").join(LIBRARY));
        out.push(prefix.join("lib").join("GodwinMix").join("onnxruntime").join(LIBRARY));
        out.push(prefix.join("lib").join("godwinmix").join(LIBRARY));
        if let Some(c) = crate::plugin::first_party::checkout_of(&exe_dir) {
            out.push(c.join("target").join("onnxruntime").join(LIBRARY));
        }
    }
    #[cfg(target_os = "macos")]
    out.push(PathBuf::from("/Applications/GodwinMix.app/Contents/Resources/onnxruntime").join(LIBRARY));
    // Last, the bare name: a runtime installed system wide.
    out.push(PathBuf::from(LIBRARY));
    out
}

