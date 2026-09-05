fn main() {
    // libcef.so and the CEF resources are staged next to the binary, so the
    // binary finds them without LD_LIBRARY_PATH.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
}
