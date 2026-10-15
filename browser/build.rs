fn main() {
    // libcef.so and the CEF resources are staged next to the binary, so the
    // binary finds them without LD_LIBRARY_PATH.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
    // The release points this binary's GStreamer at the copy inside the app
    // with install_name_tool, and those paths are longer than the ones it was
    // linked with. Without room in the header only two of seven could be
    // changed. The mixer's own build.rs does the same.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-bins=-Wl,-headerpad_max_install_names");
    }
    // Chromium will not start on Windows without a manifest that says Windows
    // 10 or newer is supported. See godwinmix-browser.exe.manifest.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("godwinmix-browser.exe.manifest");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    }
}
