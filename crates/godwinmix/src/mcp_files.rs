//! A file an agent names on its own machine, carried to the mixer.
//!
//! An agent writes `lower-third.svg` in its working folder and calls
//! `save_graphic {"file": "lower-third.svg"}`. The path means something on
//! the agent's machine, not on the mixer's, and the mixer may be somewhere
//! else entirely. So before the call goes out, a path that exists here is
//! made absolute when the mixer is on this machine, and is read and sent as
//! `data` when it is not: a file as itself, a folder as a stored zip. The
//! mixer's method is the same either way; this only saves the agent from
//! knowing where the mixer is.

use base64::Engine;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// The arguments of `tool`, with any local path carried.
pub fn carry(tool: &str, args: &Value, base: &str) -> Value {
    let Some(map) = args.as_object() else { return args.clone() };
    let mut out = map.clone();
    let local = is_local(base);
    match tool {
        "save_graphic" | "import_graphics" => {
            for key in ["file", "path"] {
                if let Some(p) = out.get(key).and_then(Value::as_str).and_then(here) {
                    out.remove(key);
                    match local {
                        true => {
                            out.insert(if tool == "save_graphic" { "file" } else { "path" }.into(), json!(p.display().to_string()));
                        }
                        false => attach(&mut out, &p),
                    }
                }
            }
            if let Some(set) = out.get_mut("set").and_then(Value::as_object_mut) {
                for key in ["background", "foreground"] {
                    if let Some(p) = set.get(key).and_then(Value::as_str).and_then(here) {
                        let v = if local { json!(p.display().to_string()) } else { data_uri(&p).map(Value::String).unwrap_or(Value::Null) };
                        set.insert(key.into(), v);
                    }
                }
            }
        }
        _ => {}
    }
    Value::Object(out)
}

/// The path, absolute, when it names something on this machine.
fn here(p: &str) -> Option<PathBuf> {
    let path = Path::new(p.trim());
    if p.trim().is_empty() || p.contains("://") || !path.exists() {
        return None;
    }
    std::path::absolute(path).ok()
}

/// True when the mixer's address is this machine's loopback.
fn is_local(base: &str) -> bool {
    let host = base.split("://").nth(1).unwrap_or(base).split(['/', ':']).next().unwrap_or_default();
    matches!(host, "127.0.0.1" | "localhost" | "[" | "::1") || base.contains("[::1]")
}

/// Read `p` into `data` and `filename`: a file as it is, a folder as a zip.
fn attach(out: &mut Map<String, Value>, p: &Path) {
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "graphic".into());
    let (bytes, filename) = if p.is_dir() { (zip_folder(p), format!("{name}.zip")) } else { (std::fs::read(p).unwrap_or_default(), name) };
    out.insert("data".into(), json!(base64::engine::general_purpose::STANDARD.encode(bytes)));
    out.insert("filename".into(), json!(filename));
}

fn data_uri(p: &Path) -> Option<String> {
    let bytes = std::fs::read(p).ok()?;
    Some(format!("data:application/octet-stream;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

fn zip_folder(dir: &Path) -> Vec<u8> {
    let mut zip = godwinmix_core::zip::Zip::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if let (Ok(rel), Ok(bytes)) = (path.strip_prefix(dir), std::fs::read(&path)) {
                zip.add(&rel.to_string_lossy().replace('\\', "/"), &bytes);
            }
        }
    }
    zip.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_path_goes_as_a_path_to_a_mixer_here_and_as_bytes_to_one_elsewhere() {
        let dir = std::env::temp_dir().join(format!("gmx-carry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("strap.svg");
        std::fs::write(&file, "<svg/>").unwrap();
        let args = json!({"name": "Strap", "file": file.display().to_string()});
        let here = carry("save_graphic", &args, "http://127.0.0.1:8080");
        assert!(Path::new(here["file"].as_str().unwrap()).is_absolute());
        let away = carry("save_graphic", &args, "http://10.0.0.9:8080");
        assert_eq!(away["filename"], "strap.svg");
        assert_eq!(away["data"], base64::engine::general_purpose::STANDARD.encode("<svg/>"));
        assert!(away.get("file").is_none());
        let folder = carry("import_graphics", &json!({"path": dir.display().to_string()}), "http://10.0.0.9:8080");
        assert!(folder["filename"].as_str().unwrap().ends_with(".zip"));
        let untouched = carry("save_graphic", &json!({"name": "x", "file": "no/such/file.svg"}), "http://10.0.0.9:8080");
        assert_eq!(untouched["file"], "no/such/file.svg", "a path that is not here is the mixer's to read");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
