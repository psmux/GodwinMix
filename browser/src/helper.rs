//! The helper executable for macOS app bundles.
//!
//! On macOS CEF cannot fork the main binary for its renderer, GPU and utility
//! processes; each runs a separate helper app inside the bundle. This is that
//! helper: load the framework, hand control to CEF, exit with its code.

#[cfg(target_os = "macos")]
fn main() {
    let exe = std::env::current_exe().expect("own path");
    let loader = cef::library_loader::LibraryLoader::new(&exe, true);
    if !loader.load() {
        eprintln!("[browser helper] could not load the Chromium Embedded Framework");
        std::process::exit(2);
    }
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = cef::args::Args::new();
    let code = cef::execute_process(Some(args.as_main_args()), None::<&mut cef::App>, std::ptr::null_mut());
    std::process::exit(code);
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("this helper is only used on macOS");
}
