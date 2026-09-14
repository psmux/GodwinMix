//! The crash report.
//!
//! A sidecar that panics is a source that went away, and the operator needs to
//! know why without reproducing it. The hook writes one JSON file into
//! `GMX_PLUGIN_ROOT/crashes/` with the panic message, the backtrace and the
//! last fifty log lines this process produced, then prints the path on stderr
//! as a `log` notification so the core can attach it to
//! `event/plugin.state.detail`.
//!
//! The log ring is filled by [`crate::framing::Writer::log`], so a plugin that
//! logs through the SDK gets its own last words in the report for free.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::wire::LogLevel;

/// How many log lines a report carries.
pub const LOG_RING: usize = 50;

static RING: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Put one line in the ring. Called by the SDK's own logging.
pub fn record_log(level: LogLevel, message: &str) {
    let line = format!("[{}] {}", level.as_str(), message);
    let mut ring = match RING.lock() {
        Ok(r) => r,
        Err(e) => e.into_inner(),
    };
    if ring.len() == LOG_RING {
        ring.remove(0);
    }
    ring.push(line);
}

/// The lines the ring holds, oldest first.
pub fn recent_logs() -> Vec<String> {
    match RING.lock() {
        Ok(r) => r.clone(),
        Err(e) => e.into_inner().clone(),
    }
}

/// Empty the ring. Tests use this; a plugin has no reason to.
pub fn clear_logs() {
    match RING.lock() {
        Ok(mut r) => r.clear(),
        Err(e) => e.into_inner().clear(),
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Write one report. Returns the path it landed at.
///
/// Failing to write a crash report must never itself panic, so every error here
/// turns into `None` and a line on stderr.
pub fn write_report(dir: &Path, message: &str, backtrace: &str) -> Option<PathBuf> {
    if let Err(e) = std::fs::create_dir_all(dir) {
        let _ = writeln!(std::io::stderr(), "could not create {}: {e}", dir.display());
        return None;
    }
    let path = dir.join(format!("crash-{}.json", now_ms()));
    let report = serde_json::json!({
        "kind": "crash",
        "ts_ms": now_ms(),
        "plugin": std::env::var("GMX_PLUGIN").unwrap_or_default(),
        "provide": std::env::var("GMX_PROVIDE").unwrap_or_default(),
        "instance": std::env::var("GMX_INSTANCE").unwrap_or_default(),
        "message": message,
        "backtrace": backtrace,
        "logs": recent_logs(),
    });
    let body = serde_json::to_string_pretty(&report).ok()?;
    match std::fs::write(&path, body) {
        Ok(()) => Some(path),
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "could not write {}: {e}", path.display());
            None
        }
    }
}

/// Install the panic hook. Call it first in `main`.
///
/// The previous hook still runs, so a plugin that installed its own keeps it.
pub fn install(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = panic_message(info);
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        if let Some(path) = write_report(&dir, &message, &backtrace) {
            // A `log` notification, written straight to stderr: the Writer may
            // be the thing that is poisoned.
            let line = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "log",
                "params": {
                    "level": "error",
                    "message": format!("plugin panicked: {message}"),
                    "crash_report": path.to_string_lossy(),
                }
            });
            let _ = writeln!(std::io::stderr(), "{line}");
        }
        previous(info);
    }));
}

/// Install the hook using `GMX_PLUGIN_ROOT` from the environment.
pub fn install_from_env() {
    install(crate::env::PluginEnv::from_env().crash_dir());
}

fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    let text = if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "a panic with a payload that is not a string".to_string()
    };
    match info.location() {
        Some(loc) => format!("{text} at {}:{}:{}", loc.file(), loc.line(), loc.column()),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The log ring is one per process, so the tests that assert on its
    /// contents take turns.
    static RING_TESTS: Mutex<()> = Mutex::new(());

    // The ring is one per process and the other tests in this crate log into
    // it while these run, so these assert what stays true either way: the ring
    // never grows past its bound, the newest line is there, and the oldest is
    // gone.
    #[test]
    fn the_ring_keeps_the_last_fifty_lines() {
        let _turn = RING_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear_logs();
        for i in 0..LOG_RING + 10 {
            record_log(LogLevel::Info, &format!("line {i}"));
        }
        let logs = recent_logs();
        assert!(logs.len() <= LOG_RING, "the ring grew to {}", logs.len());
        assert!(
            logs.iter().any(|l| l == "[info] line 59"),
            "the newest line was dropped: {logs:?}"
        );
        assert!(
            !logs.iter().any(|l| l == "[info] line 0"),
            "the oldest line was kept: {logs:?}"
        );
    }

    #[test]
    fn a_report_carries_the_message_the_backtrace_and_the_logs() {
        let _turn = RING_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        clear_logs();
        record_log(LogLevel::Warn, "the camera went away");
        let dir = std::env::temp_dir().join(format!("gmx-sdk-crash-{}", now_ms()));
        let path = write_report(&dir, "it broke at main.rs:10", "frame 0\nframe 1").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["message"], "it broke at main.rs:10");
        assert_eq!(value["backtrace"], "frame 0\nframe 1");
        let logs = value["logs"].as_array().expect("no logs in the report");
        assert!(
            logs.iter().any(|l| l == "[warn] the camera went away"),
            "the report lost the log line: {logs:?}"
        );
        assert!(path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("crash-"));
        let _ = std::fs::remove_dir_all(&dir);
        clear_logs();
    }

    #[test]
    fn an_unwritable_directory_does_not_panic() {
        // A path under a file is never a directory, so create_dir_all fails.
        let file = std::env::temp_dir().join(format!("gmx-sdk-not-a-dir-{}", now_ms()));
        std::fs::write(&file, b"x").unwrap();
        let dir = file.join("crashes");
        assert!(write_report(&dir, "m", "b").is_none());
        let _ = std::fs::remove_file(&file);
    }
}
