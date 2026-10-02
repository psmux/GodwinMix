//! A child process with its output in a log file.
//!
//! A build prints thousands of lines nobody operating a show needs to read.
//! They go to `<home>/logs/setup-<piece>.log`, appended, with a header naming
//! the command, and the mixer's own log says where. Nothing here blocks a
//! thread: the child is awaited on the runtime.

use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Run `cmd` to the end with its output appended to `log`. Err says how it
/// ended, for the detail of a refusal.
pub async fn logged(mut cmd: Command, log: &Path) -> Result<(), String> {
    use std::io::Write;
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| format!("opening {}: {e}", log.display()))?;
    let _ = writeln!(file, "\n==> {:?}", cmd.as_std());
    let err = file.try_clone().map_err(|e| e.to_string())?;
    cmd.stdin(Stdio::null()).stdout(Stdio::from(file)).stderr(Stdio::from(err)).kill_on_drop(true);
    let status = cmd.spawn().map_err(|e| format!("starting {:?}: {e}", cmd.as_std().get_program()))?.wait().await;
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("{:?} ended with {s}; the log is {}", cmd.as_std().get_program(), log.display())),
        Err(e) => Err(format!("waiting on {:?}: {e}", cmd.as_std().get_program())),
    }
}

/// Where `name` is on `PATH`, when it is.
pub fn on_path(name: &str) -> Option<std::path::PathBuf> {
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(&exe)).find(|p| p.is_file())
}
