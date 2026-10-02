#[test]
fn the_pinned_version_is_the_lock_files_metadata() {
    let lock = "[[package]]\nname = \"cef-dll-sys\"\nversion = \"150.0.0+150.0.10\"\nsource = \"x\"\n";
    assert_eq!(super::pinned(lock).as_deref(), Some("150.0.10"));
    assert_eq!(super::pinned("name = \"cef\"\nversion = \"1\"\n"), None);
}

#[test]
fn this_machine_has_a_folder_name_the_build_looks_for() {
    if let Some((key, dir)) = super::platform() {
        assert!(dir.starts_with("cef_") && !key.is_empty());
    }
}
