//! Whether GStreamer GL may be started in this process at all.
//!
//! Not on Windows, unless asked for. On an Intel laptop (Arc graphics,
//! driver 32.0.101.8860) a thread of the GL driver's own read a null pointer
//! (`igxelpgicd64.dll` at offset `0xd3c80`) and took the whole show down,
//! twice in one afternoon, about ten seconds after the first `fx.list` made
//! the probe start GL. A show always has hardware video in it by then: the
//! D3D12 decoder for an H.264 source and the Media Foundation encoders. A
//! test that started GL beside them crashed the same way, at the same offset,
//! in 11 runs of 15 (3 of 9 beside the D3D12 decoder alone, 5 of 9 beside
//! the Media Foundation encoder alone), and in none of 12 beside a software
//! decode. The fault is in the driver, so the cure here is not to load it:
//! shaders run their software version, or as a dissolve, and the programme
//! never shares a process with the GL driver.
//!
//! `GODWINMIX_FX_GL=on` in the mixer's environment asks for the GPU anyway,
//! on a Windows machine whose driver is known to be good, and `off` refuses
//! it on any platform.

/// The switch, read from the mixer's environment.
pub const SWITCH_ENV: &str = "GODWINMIX_FX_GL";

/// Whether this process may start GStreamer GL for a shader.
pub fn allowed() -> bool {
    decide(std::env::var(SWITCH_ENV).ok().as_deref(), cfg!(windows))
}

/// The rule, apart from where its inputs come from: the switch wins, and
/// with none GL runs everywhere but Windows.
fn decide(switch: Option<&str>, windows: bool) -> bool {
    match switch.map(|s| s.trim().to_ascii_lowercase()) {
        Some(s) if matches!(s.as_str(), "on" | "1" | "true" | "yes") => true,
        Some(s) if matches!(s.as_str(), "off" | "0" | "false" | "no") => false,
        _ => !windows,
    }
}

/// Why the GPU is not used, for the log line and an error that names the
/// switch.
pub fn reason() -> String {
    if cfg!(windows) && std::env::var(SWITCH_ENV).is_err() {
        format!("GPU shaders are off on Windows, where the Intel GL driver crashed the show beside the hardware video decoder; shaders run the software way. Set {SWITCH_ENV}=on to try the GPU on this machine")
    } else {
        format!("GPU shaders are off because {SWITCH_ENV} says so; shaders run the software way")
    }
}

#[cfg(test)]
mod tests {
    use super::decide;

    #[test]
    fn windows_runs_shaders_in_software_unless_the_switch_asks_for_the_gpu() {
        assert!(!decide(None, true), "no switch on Windows: no GL");
        assert!(decide(None, false), "no switch elsewhere: GL as before");
        assert!(decide(Some("on"), true), "on asks for the GPU on Windows");
        assert!(decide(Some(" TRUE "), true), "the switch reads case and spaces loosely");
        assert!(!decide(Some("off"), false), "off refuses it anywhere");
        assert!(!decide(Some("maybe"), true), "a word it does not know leaves the default");
        assert!(decide(Some("maybe"), false), "a word it does not know leaves the default");
    }
}
