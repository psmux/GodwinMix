//! Validate `gmx-plugin.toml` with the SDK's own validator.
//!
//! The same code the conformance harness runs for check 7, and the same
//! messages. Every problem is reported with the key path that caused it, so a
//! whole file is fixed in one pass rather than one error per run.
//!
//!     cargo run --quiet --bin check-manifest

use godwinmix_sdk::manifest::Manifest;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "gmx-plugin.toml".to_string());
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("could not read {path}: {e}");
            std::process::exit(1);
        }
    };
    let manifest = match Manifest::parse(&text) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let root = std::path::Path::new(&path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let problems = manifest.validate(Some(root));
    if problems.is_empty() {
        println!(
            "ok: {} {} at api {}, {} provide(s)",
            manifest.plugin.name,
            manifest.plugin.version,
            manifest.plugin.api,
            manifest.provides.len()
        );
        return;
    }
    eprintln!("{path} has {} problems:", problems.len());
    for problem in &problems {
        eprintln!("  {problem}");
    }
    std::process::exit(1);
}
