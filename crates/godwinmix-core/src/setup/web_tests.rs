use super::*;

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-web-lookup-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A checkout: `Cargo.toml`, `plugins/`, `browser/Cargo.toml`, and the
/// mixer built in `target/release`.
fn checkout(tag: &str) -> (PathBuf, PathBuf) {
    let root = root(tag);
    std::fs::write(root.join("Cargo.toml"), "").unwrap();
    std::fs::create_dir_all(root.join("plugins")).unwrap();
    std::fs::create_dir_all(root.join("browser")).unwrap();
    std::fs::write(root.join("browser/Cargo.toml"), "").unwrap();
    let exe_dir = root.join("target/release");
    std::fs::create_dir_all(&exe_dir).unwrap();
    (root, exe_dir)
}

fn touch(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b"").unwrap();
}

#[test]
fn a_checkout_with_nothing_built_can_build_it() {
    let (root, exe_dir) = checkout("unbuilt");
    let found = lookup_from(&BrowserConfig::default(), Some(&exe_dir), None);
    assert!(found.found.is_none());
    assert_eq!(found.buildable.as_deref(), Some(root.join("browser").as_path()));
    assert!(found.looked.contains(&checkout_place(&root)), "the checkout's own build is looked at");
}

#[test]
fn the_checkouts_own_build_is_found_where_its_build_leaves_it() {
    let (root, exe_dir) = checkout("built");
    let place = checkout_place(&root);
    if cfg!(target_os = "macos") {
        assert!(place.ends_with("browser/target/release/godwinmix-browser.app/Contents/MacOS/godwinmix-browser"));
        // Built and not bundled does not run on macOS, so it is not taken.
        touch(&root.join("browser/target/release/godwinmix-browser"));
        assert!(lookup_from(&BrowserConfig::default(), Some(&exe_dir), None).found.is_none());
    }
    touch(&place);
    assert_eq!(lookup_from(&BrowserConfig::default(), Some(&exe_dir), None).found, Some(place));
}

#[test]
fn one_beside_the_mixer_wins_over_the_checkout() {
    let (root, exe_dir) = checkout("beside");
    touch(&checkout_place(&root));
    let beside = packaged_places(&exe_dir).remove(0);
    touch(&beside);
    assert_eq!(lookup_from(&BrowserConfig::default(), Some(&exe_dir), None).found, Some(beside));
}

#[test]
fn a_package_is_looked_at_where_each_platform_puts_it() {
    let exe_dir = Path::new("/opt/gmx/bin");
    let places = packaged_places(exe_dir);
    if cfg!(target_os = "macos") {
        assert!(places.iter().any(|p| p.starts_with("/opt/gmx/Resources/godwinmix-browser.app")));
        // Where the desktop app's `browser` resource lands.
        assert!(places.iter().any(|p| p.starts_with("/opt/gmx/Resources/browser/godwinmix-browser.app")));
    } else {
        assert!(places.iter().any(|p| p.starts_with("/opt/gmx/bin/browser")));
        assert!(places.iter().any(|p| p.starts_with("/opt/gmx/lib/GodwinMix/browser")));
    }
}

#[test]
fn a_configured_path_that_is_not_there_is_said_and_nothing_else_is_tried() {
    let (_root, exe_dir) = checkout("configured");
    let cfg = BrowserConfig { sidecar: Some("/nowhere/godwinmix-browser.app".into()), ..Default::default() };
    let found = lookup_from(&cfg, Some(&exe_dir), None);
    assert!(found.found.is_none() && found.buildable.is_none());
    assert_eq!(
        found.configured_missing,
        Some(PathBuf::from("/nowhere/godwinmix-browser.app/Contents/MacOS/godwinmix-browser"))
    );
    // An empty setting is no setting.
    let empty = BrowserConfig { sidecar: Some(" ".into()), ..Default::default() };
    assert!(lookup_from(&empty, Some(&exe_dir), None).buildable.is_some());
}

#[test]
fn outside_a_checkout_nothing_is_buildable() {
    let exe_dir = root("loose");
    let found = lookup_from(&BrowserConfig::default(), Some(&exe_dir), None);
    assert!(found.found.is_none() && found.buildable.is_none());
}
