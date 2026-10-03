//! The drift test: `src/generated.rs` against `protocol.json`.
//!
//! A method added to the core changes `protocol.json`, and this fails until
//! someone runs `python3 clients/gen/generate.py` and commits what changed.
//! That is the whole mechanism by which a core method reaches this library.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <repo>/crates/godwinmix-client.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The first of python3 and python that answers. On Windows `python3` can be
/// the Microsoft Store's stand-in, which exists and fails, so a name that
/// merely starts is not enough.
fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|p| Command::new(p).arg("--version").output().is_ok_and(|o| o.status.success()))
}

#[test]
fn generated_rust_matches_protocol_json() {
    let root = repo_root();
    let generator = root.join("clients/gen/generate.py");
    if !generator.exists() {
        // Published as a crate on its own, without the repository around it.
        eprintln!("no clients/gen/generate.py here, so there is nothing to check against");
        return;
    }
    let Some(python) = python() else {
        eprintln!("no Python answered, so the drift check was skipped");
        return;
    };
    let run = Command::new(python)
        .arg(&generator)
        .arg("--check")
        .arg("--lang")
        .arg("rust")
        .current_dir(&root)
        .output();
    let out = match run {
        Ok(out) => out,
        Err(e) => {
            eprintln!("{python} did not run ({e}), so the drift check was skipped");
            return;
        }
    };
    assert!(
        out.status.success(),
        "src/generated.rs is stale. Run `python3 clients/gen/generate.py` and commit the result.\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_protocol_file_is_the_one_this_build_was_generated_from() {
    let root = repo_root();
    let path = root.join("protocol.json");
    if !path.exists() {
        return;
    }
    let text = std::fs::read_to_string(path).expect("protocol.json reads");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("protocol.json parses");
    assert_eq!(doc["api_level"].as_u64(), Some(godwinmix_client::API_LEVEL as u64));
    let methods = doc["methods"].as_array().expect("a methods list");
    assert_eq!(methods.len(), godwinmix_client::METHODS.len());
}
