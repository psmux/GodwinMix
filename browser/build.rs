use std::path::{Path, PathBuf};

fn main() {
    // libcef.so and the CEF resources are staged next to the binary, so the
    // binary finds them without LD_LIBRARY_PATH.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os == "linux" || os == "windows" {
        stage_cef_beside_the_binary();
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
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}

/// Put libcef and its resources beside the binary, where the binary and every
/// packaging script look for them.
///
/// cef-dll-sys copies them to three folders above its own `OUT_DIR`, which
/// was `target/release` until Cargo's new build directory layout added a
/// level (`target/release/build/<package>/<hash>/out`). From then on they
/// landed in `target/release/build`, the binary would not start, and the
/// container image and the Linux and Windows packages stopped on a missing
/// `locales`. The profile folder is found here from this package's own
/// `OUT_DIR` in either layout, and the files are copied when they are not
/// already there at the same size.
fn stage_cef_beside_the_binary() {
    let Ok(cef_dir) = std::env::var("DEP_CEF_DLL_WRAPPER_CEF_DIR") else {
        println!("cargo::warning=cef-dll-sys did not say where CEF is, so nothing was staged beside the binary");
        return;
    };
    let cef_dir = PathBuf::from(cef_dir);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let Some(profile) = profile_dir(&out_dir) else {
        println!(
            "cargo::warning=no build folder above {}, so CEF was not staged",
            out_dir.display()
        );
        return;
    };
    println!("cargo::rerun-if-changed={}", cef_dir.display());
    if let Err(e) = copy_files(&cef_dir, &profile)
        .and_then(|()| copy_files(&cef_dir.join("locales"), &profile.join("locales")))
    {
        panic!(
            "staging CEF from {} into {}: {e}",
            cef_dir.display(),
            profile.display()
        );
    }
}

/// `target/<profile>`: the folder that holds the `build` folder this
/// `OUT_DIR` is inside, in the old layout and the new one.
fn profile_dir(out_dir: &Path) -> Option<PathBuf> {
    out_dir
        .ancestors()
        .find(|d| d.file_name().is_some_and(|n| n == "build"))
        .and_then(|b| b.parent())
        .map(Path::to_path_buf)
}

fn copy_files(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        if !src.is_file() {
            continue;
        }
        let dest = to.join(entry.file_name());
        let same = match (std::fs::metadata(&src), std::fs::metadata(&dest)) {
            (Ok(a), Ok(b)) => a.len() == b.len(),
            _ => false,
        };
        if !same {
            let _ = std::fs::remove_file(&dest);
            std::fs::copy(&src, &dest)?;
        }
    }
    Ok(())
}
