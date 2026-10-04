//! On Windows, load the GStreamer and GLib DLLs on first use rather than at
//! start, so the installed `godwinmix.exe` can find the copy the installer put
//! beside it before Windows goes looking. See `src/bundled.rs`.

/// What `godwinmix.exe` imports from GStreamer and GLib, read off the binary.
const DELAYED: &[&str] = &[
    "glib-2.0-0.dll",
    "gobject-2.0-0.dll",
    "gio-2.0-0.dll",
    "gstreamer-1.0-0.dll",
    "gstbase-1.0-0.dll",
    "gstapp-1.0-0.dll",
    "gstvideo-1.0-0.dll",
    "gstpbutils-1.0-0.dll",
    "gstcontroller-1.0-0.dll",
    "gstnet-1.0-0.dll",
    "gstsdp-1.0-0.dll",
    "gstwebrtc-1.0-0.dll",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("TARGET").unwrap_or_default().ends_with("windows-msvc") {
        for dll in DELAYED {
            println!("cargo:rustc-link-arg-bins=/DELAYLOAD:{dll}");
        }
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
    }
}
