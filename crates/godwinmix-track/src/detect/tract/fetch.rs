//! Model files: where they live, fetching one on first use, and checking it
//! against its SHA256 every time it is loaded, so a changed file upstream
//! fails here and not during a service.

use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A model the detector can run: where to get it and what it must hash to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelSpec {
    /// The file name in the models directory.
    pub file: &'static str,
    pub url: &'static str,
    /// Lower case hex.
    pub sha256: &'static str,
    /// The licence, for the record and for the reference page.
    pub licence: &'static str,
}

/// YOLOX nano, COCO trained, from the YOLOX 0.1.1rc0 release. Apache 2.0.
pub const YOLOX_NANO: ModelSpec = ModelSpec {
    file: "yolox_nano.onnx",
    url: "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_nano.onnx",
    sha256: "c789161ed43c8269fcd4e67c67eeeb4e80c622da2eb296a20bc6007bd18a0b7d",
    licence: "Apache-2.0",
};

/// Why a model could not be had.
#[derive(Debug)]
pub enum ModelError {
    /// The download failed: offline, or the address moved.
    Fetch { url: String, path: PathBuf, reason: String },
    /// The file is there but is not the file expected.
    Hash { path: PathBuf, expected: String, got: String },
    Io { path: PathBuf, reason: String },
    /// tract could not load or plan the model.
    Load { path: PathBuf, reason: String },
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::Fetch { url, path, reason } => write!(
                f,
                "could not download the person model from {url} ({reason}); connect this machine \
                 to the internet once, or copy the file to {} by hand, or use the motion backend",
                path.display()
            ),
            ModelError::Hash { path, expected, got } => write!(
                f,
                "{} has SHA256 {got}, not the expected {expected}; delete it so it is fetched \
                 again, and if it keeps happening the file upstream has changed",
                path.display()
            ),
            ModelError::Io { path, reason } => {
                write!(f, "could not read or write {} ({reason}); check the directory is writable", path.display())
            }
            ModelError::Load { path, reason } => write!(
                f,
                "tract could not load {} ({reason}); use an input size that is a multiple of 32, \
                 and if that is not it, delete the file so a fresh copy is fetched",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ModelError {}

/// The models directory: `GODWINMIX_MODELS` when set, otherwise `models`
/// under `GODWINMIX_HOME`, otherwise `~/.godwinmix/models`.
pub fn models_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("GODWINMIX_MODELS") {
        return PathBuf::from(d);
    }
    if let Some(h) = std::env::var_os("GODWINMIX_HOME") {
        return PathBuf::from(h).join("models");
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).unwrap_or_default();
    PathBuf::from(home).join(".godwinmix").join("models")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, bytes);
    d.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// The model's bytes, checked: read from `dir`, or fetched there first.
pub fn ensure_model(spec: &ModelSpec, dir: &Path) -> Result<Vec<u8>, ModelError> {
    let path = dir.join(spec.file);
    let io = |e: std::io::Error| ModelError::Io { path: path.clone(), reason: e.to_string() };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let b = download(spec.url, &path)?;
            std::fs::create_dir_all(dir).map_err(io)?;
            let tmp = path.with_extension("part");
            std::fs::write(&tmp, &b).map_err(io)?;
            check(spec, &path, &b)?;
            std::fs::rename(&tmp, &path).map_err(io)?;
            return Ok(b);
        }
        Err(e) => return Err(io(e)),
    };
    check(spec, &path, &bytes)?;
    Ok(bytes)
}

fn check(spec: &ModelSpec, path: &Path, bytes: &[u8]) -> Result<(), ModelError> {
    let got = sha256_hex(bytes);
    if got == spec.sha256 {
        return Ok(());
    }
    Err(ModelError::Hash { path: path.to_path_buf(), expected: spec.sha256.into(), got })
}

fn download(url: &str, path: &Path) -> Result<Vec<u8>, ModelError> {
    let fail = |reason: String| ModelError::Fetch { url: url.into(), path: path.to_path_buf(), reason };
    let resp = ureq::get(url).call().map_err(|e| fail(e.to_string()))?;
    let mut buf = Vec::new();
    resp.into_body()
        .into_reader()
        .take(64 << 20)
        .read_to_end(&mut buf)
        .map_err(|e| fail(e.to_string()))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrong_file_is_refused_with_both_hashes() {
        let dir = std::env::temp_dir().join(format!("gmx-track-hash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(YOLOX_NANO.file), b"not a model").unwrap();
        let e = ensure_model(&YOLOX_NANO, &dir).unwrap_err().to_string();
        std::fs::remove_dir_all(&dir).ok();
        assert!(e.contains(YOLOX_NANO.sha256) && e.contains("delete it"), "{e}");
    }
}
