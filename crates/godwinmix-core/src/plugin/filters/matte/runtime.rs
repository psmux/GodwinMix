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
use std::sync::{Mutex, OnceLock};

/// The library's file name on this platform.
pub const LIBRARY: &str = if cfg!(windows) {
    "onnxruntime.dll"
} else if cfg!(target_os = "macos") {
    "libonnxruntime.dylib"
} else {
    "libonnxruntime.so"
};

static LOADED: OnceLock<PathBuf> = OnceLock::new();
static LOADING: Mutex<()> = Mutex::new(());

/// Cache a successful load. A missing runtime can be installed and retried
/// without forcing the operator to restart a running programme.
pub fn load() -> Result<&'static Path> {
    if let Some(path) = LOADED.get() {
        return Ok(path.as_path());
    }
    let _guard = LOADING.lock().unwrap_or_else(|e| e.into_inner());
    if LOADED.get().is_none() {
        let path = super::discovery::locate().map_err(|why| anyhow!("{why}"))?;
        let _ = LOADED.set(path);
    }
    Ok(LOADED.get().expect("loaded above").as_path())
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
