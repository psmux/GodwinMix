use super::*;

#[test]
fn auto_takes_the_fine_model_only_with_a_gpu() {
    let dir = std::env::temp_dir().join(format!("gmx-models-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("selfie.onnx"), b"x").unwrap();
    std::fs::write(dir.join("modnet.onnx"), b"x").unwrap();
    std::env::set_var("GODWINMIX_MODELS_DIR", &dir);
    assert_eq!(choose(&Quality::Auto, true).unwrap().name, "fine");
    assert_eq!(choose(&Quality::Auto, false).unwrap().name, "fast");
    let fine = choose(&Quality::Fine, false).unwrap();
    assert_eq!((fine.width, fine.height, fine.mean), (512, 288, 0.5));

    std::fs::write(dir.join("mine.onnx"), b"x").unwrap();
    let err = choose(&Quality::Named("mine".into()), true).unwrap_err();
    assert!(format!("{err:#}").contains("mine.json"), "{err:#}");
    std::fs::write(dir.join("mine.json"), r#"{"width": 320, "height": 192, "mean": 0.5, "std": 0.5}"#).unwrap();
    let mine = choose(&Quality::Named("mine".into()), true).unwrap();
    assert_eq!((mine.width, mine.height), (320, 192));
    let err = choose(&Quality::Named("absent".into()), true).unwrap_err();
    assert!(format!("{err:#}").contains("models folder"), "{err:#}");
    std::env::remove_var("GODWINMIX_MODELS_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cached_and_installed_models_are_discoverable_without_environment_settings() {
    let dirs = places();
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        assert!(dirs.contains(&Path::new(&home).join(".cache/gmx-models")));
    }
    #[cfg(target_os = "macos")]
    assert!(dirs.contains(&PathBuf::from("/Applications/GodwinMix.app/Contents/Resources/models")));
}
