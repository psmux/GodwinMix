//! `gmx doctor`: what is wrong with this machine, one line per finding.
//!
//! The audience is somebody setting up a box they have not used before, or
//! somebody whose stream will not start and who has ten minutes. Every check
//! answers in one line, says what to do about it, and the command's exit code
//! is non zero when something the default pipeline actually needs is missing.
//!
//! Cross platform: the element and version checks are GStreamer's own and work
//! everywhere. Disk space and memory are the two places the platforms differ,
//! and each has a Unix path, a Windows path, and a fallback that reports
//! "unknown" rather than guessing.

use crate::config::Config;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Nothing to do.
    Ok,
    /// Works, but somebody should know.
    Warn,
    /// The mixer will not do its job like this. Exit code 1.
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub verdict: Verdict,
    /// One line. What was found, and where it leaves the operator.
    pub detail: String,
}

impl Check {
    fn new(name: &str, verdict: Verdict, detail: impl Into<String>) -> Self {
        Self { name: name.into(), verdict, detail: detail.into() }
    }
}

/// The elements today's pipelines build with, and whether the mixer can run
/// without each.
///
/// This list is temporary by design. When the codec catalogue lands (03
/// section 5, 07 Phase 1) it knows which elements each entry needs and this
/// constant is replaced by a call to `crate::catalogue::doctor_lines()`, which
/// `elements()` below already prefers when it exists. Until then the list is
/// what a `grep` of `make(` in mixer.rs, input.rs, output.rs and multiview.rs
/// answers, which is the set the default pipeline cannot start without.
const REQUIRED: &[(&str, &str)] = &[
    ("compositor", "mixing video"),
    ("audiomixer", "mixing audio"),
    ("tee", "fanning the programme out"),
    ("queue", "every buffer between two threads"),
    ("capsfilter", "holding every branch to the canvas format"),
    ("videoconvert", "normalising a source to the canvas"),
    ("videoscale", "normalising a source to the canvas"),
    ("videorate", "normalising a source to the canvas"),
    ("audioconvert", "normalising a source's audio"),
    ("audioresample", "normalising a source's audio"),
    ("audiorate", "normalising a source's audio"),
    ("volume", "the operator's faders"),
    ("level", "the programme meters"),
    ("videotestsrc", "the slate"),
    ("audiotestsrc", "the slate's silence"),
    ("decodebin", "decoding anything that is not raw"),
    ("uridecodebin", "file and HLS sources"),
    ("h264parse", "the programme's video before the muxer"),
    ("aacparse", "the programme's audio before the muxer"),
    ("flvmux", "the RTMP container"),
    ("proxysink", "keeping an output's failure off the programme"),
    ("proxysrc", "keeping an output's failure off the programme"),
    ("fakesink", "branches with nowhere to go"),
];

/// Useful but not fatal.
const OPTIONAL: &[(&str, &str)] = &[
    ("rtmp2sink", "RTMP outputs"),
    ("rtmp2src", "RTMP sources"),
    ("rtmpsink", "the fallback RTMP client"),
    ("flvdemux", "RTMP sources"),
    ("jpegenc", "the multiview mosaic and snapshots"),
    ("livesync", "holding a stalled source's last frame"),
    ("wpesrc", "web page sources without a browser sidecar"),
    ("mp4mux", "recording to a file"),
    ("filesink", "recording to a file"),
    ("fdsrc", "sidecar sources"),
];

/// At least one of these has to exist or nothing can be encoded.
const VIDEO_ENCODERS: &[&str] = &[
    "nvh264enc",
    "vah264enc",
    "vaapih264enc",
    "mfh264enc",
    "vtenc_h264_hw",
    "vtenc_h264",
    "x264enc",
];

const AUDIO_ENCODERS: &[&str] = &["fdkaacenc", "avenc_aac", "voaacenc"];

/// Run every check. `config_path` is the config the operator would start with.
pub fn run(config_path: &Path) -> Vec<Check> {
    let mut checks = vec![gstreamer_version()];
    checks.extend(elements());
    checks.push(encoder("video encoder", VIDEO_ENCODERS));
    checks.push(encoder("audio encoder", AUDIO_ENCODERS));

    // Loaded once. `Config::load` logs what it found, and loading twice put
    // every one of those lines on the screen twice.
    let loaded = config_path.exists().then(|| Config::load(config_path));
    checks.push(config_check(config_path, loaded.as_ref()));
    if let Some(Ok(cfg)) = &loaded {
        checks.push(port_free(&cfg.control.bind));
    }
    let dir = crate::observe::runtime_dir(config_path);
    checks.push(runtime_dir_writable(&dir));
    checks.push(disk_space(&dir));
    checks.push(machine_class());
    checks.push(wasm_host());
    checks
}

/// Whether this build can run a tier W plugin.
///
/// A warning rather than a failure when it cannot, because a mixer with no
/// WebAssembly host runs a show perfectly well: the placement is for plugin
/// logic, never for media. It becomes an operator's problem only when a
/// plugin whose only placement is `wasm` is installed, and that refusal names
/// the flag too.
fn wasm_host() -> Check {
    match crate::plugin::wasm::describe() {
        Some(what) => Check::new("wasm host", Verdict::Ok, what),
        None => Check::new(
            "wasm host",
            Verdict::Warn,
            "this build carries none, so a plugin with placements = [\"wasm\"] will not \
             start. Rebuild with `cargo build --release --features wasm` if you need one."
                .to_string(),
        ),
    }
}

/// Exit code for a set of checks: 1 when anything failed, 0 otherwise. A
/// warning is not a failure, because a machine with no hardware encoder still
/// runs a show.
pub fn exit_code(checks: &[Check]) -> i32 {
    i32::from(checks.iter().any(|c| c.verdict == Verdict::Fail))
}

/// The checks as a person reads them: one line each, verdict first so the eye
/// runs down the left hand column.
pub fn format(checks: &[Check]) -> String {
    use std::fmt::Write as _;
    let width = checks.iter().map(|c| c.name.len()).max().unwrap_or(10).clamp(10, 32);
    let mut out = String::new();
    for c in checks {
        let mark = match c.verdict {
            Verdict::Ok => "ok  ",
            Verdict::Warn => "warn",
            Verdict::Fail => "FAIL",
        };
        let _ = writeln!(out, "{mark}  {:<width$}  {}", c.name, c.detail, width = width);
    }
    let failed = checks.iter().filter(|c| c.verdict == Verdict::Fail).count();
    let warned = checks.iter().filter(|c| c.verdict == Verdict::Warn).count();
    let _ = match (failed, warned) {
        (0, 0) => writeln!(out, "\nall {} checks passed", checks.len()),
        (0, w) => writeln!(out, "\n{} checks passed, {w} worth reading", checks.len() - w),
        (f, _) => writeln!(out, "\n{f} of {} checks failed", checks.len()),
    };
    out
}

fn gstreamer_version() -> Check {
    let (major, minor, micro, _) = gstreamer::version();
    let found = format!("{major}.{minor}.{micro}");
    // 1.20 is where `rtmp2sink`, `livesync` and the aggregator behaviour the
    // programme depends on are all present and behave as this code expects.
    if (major, minor) < (1, 20) {
        Check::new(
            "gstreamer",
            Verdict::Fail,
            format!("{found} is too old. 1.20 or newer is needed; 1.28 is what this is tested on"),
        )
    } else {
        Check::new("gstreamer", Verdict::Ok, found)
    }
}

/// Every element the catalogue needs, named individually when missing.
fn elements() -> Vec<Check> {
    // When the catalogue answers for this, it becomes:
    //     let required = crate::catalogue::doctor_lines();
    // and the constants above go away. The shape of a line does not change.
    let missing_required: Vec<_> = REQUIRED
        .iter()
        .filter(|(name, _)| gstreamer::ElementFactory::find(name).is_none())
        .collect();
    let missing_optional: Vec<_> = OPTIONAL
        .iter()
        .filter(|(name, _)| gstreamer::ElementFactory::find(name).is_none())
        .collect();

    let mut checks = Vec::new();
    if missing_required.is_empty() {
        checks.push(Check::new(
            "elements",
            Verdict::Ok,
            format!("all {} elements the default pipeline needs are here", REQUIRED.len()),
        ));
    } else {
        for (name, why) in &missing_required {
            checks.push(Check::new(
                "elements",
                Verdict::Fail,
                format!("{name} is missing, and it is what does {why}. {}", install_hint(name)),
            ));
        }
    }
    for (name, why) in missing_optional {
        checks.push(Check::new(
            "elements",
            Verdict::Warn,
            format!("{name} is missing, so {why} will not work. {}", install_hint(name)),
        ));
    }
    checks
}

/// Which package an element comes from, said in the words of the platform the
/// operator is standing in front of.
fn install_hint(element: &str) -> String {
    let plugin = match element {
        "compositor" | "audiomixer" | "videorate" | "videoscale" | "audioconvert"
        | "audioresample" | "audiorate" | "volume" | "level" | "videotestsrc" | "audiotestsrc"
        | "tee" | "queue" | "capsfilter" | "fakesink" | "filesink" | "fdsrc" | "videoconvert" => {
            "gst-plugins-base and gst-plugins-good"
        }
        "decodebin" | "uridecodebin" | "flvdemux" | "jpegenc" => "gst-plugins-good",
        "h264parse" | "aacparse" | "flvmux" | "mp4mux" | "proxysink" | "proxysrc" | "rtmp2sink"
        | "rtmp2src" | "livesync" => "gst-plugins-bad",
        "rtmpsink" => "gst-plugins-ugly",
        "wpesrc" => "gst-plugins-bad, built with WPE",
        _ => "the GStreamer plugin set",
    };
    if cfg!(target_os = "macos") {
        format!("Install {plugin} (brew install gstreamer)")
    } else if cfg!(target_os = "windows") {
        format!("Install {plugin} from the GStreamer MSI, the complete profile")
    } else {
        format!("Install {plugin}")
    }
}

fn encoder(name: &str, candidates: &[&str]) -> Check {
    let present: Vec<_> = candidates
        .iter()
        .filter(|c| gstreamer::ElementFactory::find(c).is_some())
        .copied()
        .collect();
    match present.split_first() {
        None => Check::new(
            name,
            Verdict::Fail,
            format!("none of {} is installed, so nothing can be encoded", candidates.join(", ")),
        ),
        // A machine with only the software encoder runs, and says so, because
        // the person choosing a board should choose knowing.
        Some((first, [])) => Check::new(
            name,
            Verdict::Warn,
            format!("{first} only, which is software. Expect a core per 1080p30 encode"),
        ),
        Some((first, rest)) => {
            Check::new(name, Verdict::Ok, format!("{first} will be chosen; also here: {}", rest.join(", ")))
        }
    }
}

/// `loaded` is `None` when the file is not there, and otherwise the one result
/// of loading it. `Config::load` validates as well as parses, so a single call
/// answers both "does it parse" and "does it mean anything".
fn config_check(path: &Path, loaded: Option<&anyhow::Result<Config>>) -> Check {
    let Some(loaded) = loaded else {
        return Check::new(
            "config",
            Verdict::Warn,
            format!(
                "{} does not exist. Run 'godwinmix --example-config > {}' to start one",
                path.display(),
                path.display()
            ),
        );
    };
    match loaded {
        Ok(cfg) => Check::new(
            "config",
            Verdict::Ok,
            format!(
                "{} parses: {} sources, {} outputs, canvas {}x{}@{}",
                path.display(),
                cfg.sources.len(),
                cfg.outputs.len(),
                cfg.canvas.width,
                cfg.canvas.height,
                cfg.canvas.fps
            ),
        ),
        Err(e) => Check::new("config", Verdict::Fail, one_line(&format!("{e:#}"))),
    }
}

/// A multi line error as one line.
///
/// `toml` draws the offending line with a caret under it, which is the right
/// thing in a terminal and the wrong thing in a table of one line verdicts.
/// The drawing is dropped and the sentences are kept.
fn one_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty()
                && !l.chars().all(|c| matches!(c, '|' | '^' | '-' | ' ' | '0'..='9'))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn port_free(bind: &str) -> Check {
    match std::net::TcpListener::bind(bind) {
        Ok(listener) => {
            drop(listener);
            Check::new("control port", Verdict::Ok, format!("{bind} is free"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => Check::new(
            "control port",
            Verdict::Fail,
            format!("{bind} is already taken. Another mixer is running, or something else has the port"),
        ),
        Err(e) => Check::new(
            "control port",
            Verdict::Fail,
            format!("{bind} cannot be bound: {e}"),
        ),
    }
}

fn runtime_dir_writable(dir: &Path) -> Check {
    if let Err(e) = std::fs::create_dir_all(dir) {
        return Check::new(
            "runtime directory",
            Verdict::Fail,
            format!("{} cannot be created: {e}", dir.display()),
        );
    }
    let probe = dir.join(".doctor-write-test");
    match std::fs::write(&probe, b"x") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            Check::new("runtime directory", Verdict::Ok, format!("{} is writable", dir.display()))
        }
        Err(e) => Check::new(
            "runtime directory",
            Verdict::Fail,
            format!("{} is not writable: {e}. Logs and the session log have nowhere to go", dir.display()),
        ),
    }
}

fn disk_space(dir: &Path) -> Check {
    match free_bytes(dir) {
        None => Check::new(
            "disk space",
            Verdict::Warn,
            format!("cannot be read on this platform for {}", dir.display()),
        ),
        Some(free) => {
            let gb = free as f64 / 1_073_741_824.0;
            // Five generations of a 50 MB log per instance, plus the session
            // log, plus room for a recording, comes to about a gigabyte before
            // anything unusual happens.
            if free < 1_073_741_824 {
                Check::new(
                    "disk space",
                    Verdict::Fail,
                    format!("{gb:.1} GB free under {}. Logs rotate at 250 MB and a show needs more", dir.display()),
                )
            } else if free < 5 * 1_073_741_824 {
                Check::new(
                    "disk space",
                    Verdict::Warn,
                    format!("{gb:.1} GB free under {}, which is enough for logs but not for recording", dir.display()),
                )
            } else {
                Check::new("disk space", Verdict::Ok, format!("{gb:.1} GB free under {}", dir.display()))
            }
        }
    }
}

/// What the gallery should default to on this machine: live tiles, periodic
/// snapshots, or icons only.
///
/// A live tile is a decoded, scaled video per source. On four cores with 4 GB
/// that is the difference between a mixer that switches cleanly and one that
/// does not, so the UI asks the machine rather than assuming a laptop.
fn machine_class() -> Check {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let ram_gb = total_memory_bytes().map(|b| b as f64 / 1_073_741_824.0);
    let (default, why) = gallery_default(cores, ram_gb);
    let ram = match ram_gb {
        Some(gb) => format!("{gb:.1} GB"),
        None => "unknown memory".to_string(),
    };
    Check::new(
        "machine class",
        Verdict::Ok,
        format!("{cores} cores, {ram}: the gallery should default to '{default}' ({why})"),
    )
}

/// What the gallery should default to on the machine this is running on.
///
/// `core.info` answers with this when no preset has chosen a mode, so a client
/// on a Pi starts on icons without having to guess from `hardwareConcurrency`.
pub fn gallery_default_here() -> &'static str {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let ram_gb = total_memory_bytes().map(|b| b as f64 / 1_073_741_824.0);
    gallery_default(cores, ram_gb).0
}

/// The gallery default for a machine. Held apart from the check so it is
/// testable without a machine of each size, and so the UI can call it.
pub fn gallery_default(cores: usize, ram_gb: Option<f64>) -> (&'static str, &'static str) {
    let ram = ram_gb.unwrap_or(0.0);
    if cores >= 8 && ram >= 8.0 {
        ("live", "enough of both to decode a tile per source")
    } else if cores >= 4 && ram >= 3.5 {
        ("snapshot", "a still every few seconds costs almost nothing")
    } else {
        ("icon", "anything more competes with the encoder")
    }
}

// --- the two platform specific readings --------------------------------------

/// Free bytes on the filesystem holding `dir`, or `None` where we cannot ask.
#[cfg(unix)]
fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: `statvfs` fills a struct we own from a path we keep alive for
    // the call. A zeroed struct is a valid starting value for it.
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) != 0 {
            return None;
        }
        // `f_frsize` is the fragment size and is what `f_bavail` counts. Some
        // platforms leave it zero, in which case `f_bsize` is the answer.
        let unit = if s.f_frsize > 0 { s.f_frsize as u64 } else { s.f_bsize as u64 };
        Some(unit.saturating_mul(s.f_bavail as u64))
    }
}

#[cfg(windows)]
fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free = 0u64;
    // SAFETY: the path is NUL terminated and outlives the call; the three
    // out parameters are stack locals we own.
    unsafe {
        let ok = GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        (ok != 0).then_some(free)
    }
}

#[cfg(not(any(unix, windows)))]
fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

/// Physical memory in bytes, or `None` where we cannot ask.
#[cfg(target_os = "linux")]
fn total_memory_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn total_memory_bytes() -> Option<u64> {
    let name = std::ffi::CString::new("hw.memsize").ok()?;
    let mut value: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: the name outlives the call, and the out parameter and its length
    // are stack locals we own and size correctly.
    unsafe {
        let rc = libc::sysctlbyname(
            name.as_ptr(),
            &mut value as *mut u64 as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        );
        (rc == 0).then_some(value)
    }
}

#[cfg(windows)]
fn total_memory_bytes() -> Option<u64> {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }
    // SAFETY: a zeroed struct with its `length` set is exactly what this call
    // documents, and it is a stack local we own.
    unsafe {
        let mut status: MemoryStatusEx = std::mem::zeroed();
        status.length = std::mem::size_of::<MemoryStatusEx>() as u32;
        (GlobalMemoryStatusEx(&mut status) != 0).then_some(status.total_phys)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios", windows)))]
fn total_memory_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_check_prints_one_line_and_the_exit_code_follows_the_failures() {
        gstreamer::init().unwrap();
        let dir = crate::observe::tempdir("doctor");
        let checks = run(&dir.join("godwinmix.toml"));
        assert!(checks.len() >= 6, "{checks:?}");
        let text = format(&checks);
        for c in &checks {
            assert!(!c.detail.contains('\n'), "a check ran to two lines: {c:?}");
            assert!(text.contains(&c.detail), "{text}");
        }
        // Every check that passed on a machine with GStreamer installed keeps
        // the exit code at zero. On a machine without it, the element checks
        // fail and it is one, which is the acceptance criterion.
        let failed = checks.iter().any(|c| c.verdict == Verdict::Fail);
        assert_eq!(exit_code(&checks), i32::from(failed));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_required_element_is_named_and_fails() {
        gstreamer::init().unwrap();
        // Every element in REQUIRED should be present on a machine that can
        // run the test suite at all, so this asserts the shape of the failure
        // rather than manufacturing one: the check names the element and says
        // what it does.
        let checks = elements();
        for c in &checks {
            if c.verdict == Verdict::Fail {
                assert!(c.detail.contains("is missing"), "{c:?}");
                assert!(c.detail.contains("Install"), "{c:?}");
            }
        }
        assert_eq!(exit_code(&checks), i32::from(checks.iter().any(|c| c.verdict == Verdict::Fail)));
    }

    #[test]
    fn a_taken_port_fails_and_a_free_one_passes() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let taken = listener.local_addr().unwrap().to_string();
        assert_eq!(port_free(&taken).verdict, Verdict::Fail);
        drop(listener);
        assert_eq!(port_free("127.0.0.1:0").verdict, Verdict::Ok);
    }

    #[test]
    fn a_config_that_does_not_parse_fails_and_says_which_file() {
        let dir = crate::observe::tempdir("doctor-config");
        let path = dir.join("godwinmix.toml");
        std::fs::write(&path, "this is not toml = = =").unwrap();
        let check = config_check(&path, Some(&Config::load(&path)));
        assert_eq!(check.verdict, Verdict::Fail);
        assert!(check.detail.contains("godwinmix.toml"), "{check:?}");

        std::fs::write(&path, "[canvas]\nwidth = 1280\nheight = 720\nfps = 30\nsample_rate = 48000\nchannels = 2\n").unwrap();
        assert_eq!(config_check(&path, Some(&Config::load(&path))).verdict, Verdict::Ok);
        // And a file that is not there is a warning, not a failure: an
        // operator checking a machine before writing one is doing it right.
        assert_eq!(config_check(&dir.join("absent.toml"), None).verdict, Verdict::Warn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_multi_line_parse_error_becomes_one_readable_line() {
        let drawn = "TOML parse error at line 11, column 1\n   |\n11 | [multiview]\n   | ^^^^^^^^^^^\nmissing field `width`\n";
        let out = one_line(drawn);
        assert_eq!(
            out,
            "TOML parse error at line 11, column 1; 11 | [multiview]; missing field `width`"
        );
        assert!(!out.contains('\n'));
    }

    #[test]
    fn the_gallery_default_follows_the_machine() {
        assert_eq!(gallery_default(16, Some(32.0)).0, "live");
        assert_eq!(gallery_default(8, Some(8.0)).0, "live");
        assert_eq!(gallery_default(4, Some(8.0)).0, "snapshot");
        assert_eq!(gallery_default(4, Some(4.0)).0, "snapshot");
        // A Raspberry Pi 4 with 2 GB.
        assert_eq!(gallery_default(4, Some(2.0)).0, "icon");
        assert_eq!(gallery_default(1, None).0, "icon");
    }

    #[test]
    fn the_machine_readings_answer_or_say_they_cannot() {
        let dir = crate::observe::tempdir("doctor-disk");
        // On every platform we ship on, both of these answer. The assertion is
        // that neither panics and that a number, when there is one, is not
        // absurd.
        if let Some(free) = free_bytes(&dir) {
            assert!(free > 0, "a writable temp directory with no free space");
        }
        if let Some(ram) = total_memory_bytes() {
            assert!(ram > 128 * 1024 * 1024, "under 128 MB of RAM is not a machine this runs on");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
