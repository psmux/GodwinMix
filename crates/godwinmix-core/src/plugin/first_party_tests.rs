use super::*;

const MANIFEST: &str = r#"
[plugin]
name = "NAME"
version = "0.1.0"
api = 1
description = "A first party plugin, for the lookup tests."
license = "Apache-2.0"

[run]
bin = { "linux-x86_64" = "bin/gmx-x", "linux-aarch64" = "bin/gmx-x", "macos-aarch64" = "bin/gmx-x", "macos-x86_64" = "bin/gmx-x", "windows-x86_64" = "bin/gmx-x.exe" }
"#;

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-first-party-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn plugin(at: &Path, name: &str, build: bool, built: bool) {
    std::fs::create_dir_all(at.join("bin")).unwrap();
    let mut text = MANIFEST.replace("NAME", name);
    if build {
        text.push_str("\n[build]\ncommand = \"true\"\noutput = \"bin/gmx-x\"\n");
    }
    std::fs::write(at.join("gmx-plugin.toml"), text).unwrap();
    if built {
        let bin = if cfg!(windows) { "gmx-x.exe" } else { "gmx-x" };
        std::fs::write(at.join("bin").join(bin), b"").unwrap();
    }
}

#[test]
fn a_plugin_installed_beside_the_binary_is_found_by_name() {
    let root = root("prefix");
    let exe_dir = root.join("bin");
    std::fs::create_dir_all(&exe_dir).unwrap();
    plugin(&root.join("share/godwinmix/plugins/camera"), "camera", false, true);
    let found = find_from(&exe_dir, "camera").expect("found under share");
    assert!(found.ends_with("share/godwinmix/plugins/camera"), "{}", found.display());
    assert_eq!(find_from(&exe_dir, "screen"), None);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_checkout_plugin_is_found_when_it_is_built_or_can_be() {
    let root = root("checkout");
    std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    let exe_dir = root.join("target").join("debug");
    std::fs::create_dir_all(&exe_dir).unwrap();
    plugin(&root.join("plugins/ingest"), "ingest", true, false);
    plugin(&root.join("plugins/screen"), "screen", false, true);
    plugin(&root.join("plugins/camera"), "camera", false, false);
    plugin(&root.join("plugins/renamed"), "something-else", false, true);
    assert!(find_from(&exe_dir, "ingest").is_some(), "a [build] section is enough");
    assert!(find_from(&exe_dir, "screen").is_some(), "a built binary is enough");
    assert_eq!(find_from(&exe_dir, "camera"), None, "neither built nor buildable");
    assert_eq!(find_from(&exe_dir, "renamed"), None, "the manifest names another plugin");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_name_that_is_a_path_is_never_looked_for() {
    for name in ["", "../etc", "a/b", "a\\b", "."] {
        assert!(!is_plain_name(name), "{name:?}");
        assert_eq!(find(name), None, "{name:?}");
    }
}
