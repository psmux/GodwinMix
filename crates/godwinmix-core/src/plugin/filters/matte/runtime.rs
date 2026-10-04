//! ONNX Runtime, found and loaded at run time, and a session on the best
//! device the machine has.
//!
//! Nothing here is linked. A mixer with no runtime beside it starts and runs
//! as ever; only adding the cutout asks for one, and the refusal says where
//! it was looked for. The runtime is found once per process.
//!
//! The device order is the platform's own accelerator first, then the CPU:
//! DirectML on Windows (any GPU that does Direct3D 12, Intel, AMD or NVIDIA),
//! CoreML on macOS (the GPU and the Neural Engine), CUDA and then OpenVINO
//! where a runtime built with them is installed. A provider the runtime does
//! not carry is skipped, and the CPU is always there.

use super::model::Spec;
use super::params::Device;
use anyhow::{anyhow, bail, Result};
use ort::ep::ExecutionProviderDispatch;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The library's file name on this platform.
pub const LIBRARY: &str = if cfg!(windows) {
    "onnxruntime.dll"
} else if cfg!(target_os = "macos") {
    "libonnxruntime.dylib"
} else {
    "libonnxruntime.so"
};

static LOADED: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();

/// Load the runtime, once. Answers where it came from.
pub fn load() -> Result<&'static Path> {
    match LOADED.get_or_init(locate) {
        Ok(path) => Ok(path.as_path()),
        Err(why) => Err(anyhow!("{why}")),
    }
}

fn locate() -> std::result::Result<PathBuf, String> {
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
         installers carry it; on a server install it from https://onnxruntime.ai (1.20 or \
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
    // Last, the bare name: a runtime installed system wide.
    out.push(PathBuf::from(LIBRARY));
    out
}

/// What a session runs on, for the operator to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ran {
    Gpu(&'static str),
    Cpu,
}

impl Ran {
    pub fn label(&self) -> String {
        match self {
            Ran::Gpu(p) => format!("GPU ({p})"),
            Ran::Cpu => "CPU".to_string(),
        }
    }
}

/// The accelerators this runtime can use, best first, with their names.
pub fn accelerators() -> Vec<(&'static str, ExecutionProviderDispatch)> {
    use ort::ep::ExecutionProvider;
    let mut all: Vec<(&'static str, ExecutionProviderDispatch, bool)> = Vec::new();
    #[cfg(windows)]
    {
        let ep = ort::ep::DirectML::default();
        let ok = ep.is_available().unwrap_or(false);
        all.push(("DirectML", ep.build(), ok));
    }
    #[cfg(target_os = "macos")]
    {
        let ep = ort::ep::CoreML::default();
        let ok = ep.is_available().unwrap_or(false);
        all.push(("CoreML", ep.build(), ok));
    }
    {
        let ep = ort::ep::CUDA::default();
        let ok = ep.is_available().unwrap_or(false);
        all.push(("CUDA", ep.build(), ok));
    }
    {
        let ep = ort::ep::OpenVINO::default().with_device_type("GPU");
        let ok = ep.is_available().unwrap_or(false);
        all.push(("OpenVINO", ep.build(), ok));
    }
    all.into_iter().filter(|(_, _, ok)| *ok).map(|(n, e, _)| (n, e)).collect()
}

/// Whether a GPU accelerator is there at all, for `auto` to pick a model by.
pub fn has_gpu(device: Device) -> bool {
    device != Device::Cpu && load().is_ok() && !accelerators().is_empty()
}

/// A session for `spec` on the best device `device` allows.
pub fn session(spec: &Spec, device: Device) -> Result<(Session, Ran)> {
    load()?;
    let gpus = if device == Device::Cpu { Vec::new() } else { accelerators() };
    if device == Device::Gpu && gpus.is_empty() {
        bail!(
            "device is gpu, and this ONNX Runtime has no GPU accelerator here. Set device to \
             auto or cpu, or install a runtime built with DirectML, CoreML, CUDA or OpenVINO."
        );
    }
    let ran = gpus.first().map(|(n, _)| Ran::Gpu(n)).unwrap_or(Ran::Cpu);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    let build = || -> Result<Session> {
        let builder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("{e}"))?
            // Half the cores at most: the mixer and its encoders need the rest.
            .with_intra_threads((threads / 2).clamp(1, 4))
            .map_err(|e| anyhow!("{e}"))?;
        let mut builder = builder
            .with_execution_providers(gpus.iter().map(|(_, e)| e.clone()).collect::<Vec<_>>())
            .map_err(|e| anyhow!("{e}"))?;
        Ok(builder.commit_from_file(&spec.file)?)
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(build)) {
        Ok(Ok(session)) => Ok((session, ran)),
        Ok(Err(e)) => Err(e.context(format!("could not open the {} model at {}", spec.name, spec.file.display()))),
        Err(_) => bail!(
            "the ONNX Runtime at {} is older than 1.20 or not a runtime at all; install 1.20 or \
             newer",
            load().map(|p| p.display().to_string()).unwrap_or_default()
        ),
    }
}
