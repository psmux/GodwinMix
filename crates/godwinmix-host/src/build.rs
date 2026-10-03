//! Running a manifest's `[build]` command, the same way on every platform.
//!
//! The command is a line a person wrote, `sh build` for every first party
//! plugin. On Linux and macOS it goes to `sh -c` as it always has. Windows has
//! no `sh` on its PATH, so a command that starts with `sh` or `bash` is run with
//! the one Git for Windows installs, which anyone building from a checkout has.
//! Any other command goes to `cmd /C`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The process to start for `line`, or what to install when there is none.
pub fn command(line: &str) -> Result<Command, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let Some(first) = words.first() else {
        return Err("the [build] command is empty; write the command that builds the plugin".into());
    };
    if cfg!(windows) {
        if matches!(*first, "sh" | "bash") {
            let shell = posix_shell(first).ok_or_else(|| {
                format!(
                    "`{line}` needs {first}, which Windows does not have on its own. Install Git \
                     for Windows (it brings {first}), or build the plugin yourself and add it again."
                )
            })?;
            let mut c = Command::new(shell);
            c.args(&words[1..]);
            return Ok(c);
        }
        let mut c = Command::new("cmd");
        c.args(["/C", line]);
        return Ok(c);
    }
    let mut c = Command::new("sh");
    c.args(["-c", line]);
    Ok(c)
}

/// What `[build] output` names, on disk. On Windows a binary is written with
/// `.exe` after the name the manifest gives, so that is looked for too.
pub fn output(dir: &Path, output: &str) -> Option<PathBuf> {
    let named = dir.join(output);
    if named.is_file() {
        return Some(named);
    }
    let exe = dir.join(format!("{output}.exe"));
    (cfg!(windows) && exe.is_file()).then_some(exe)
}

/// `sh` or `bash` on PATH, else the one beside Git for Windows.
fn posix_shell(name: &str) -> Option<PathBuf> {
    crate::launch::which(name).or_else(|| git_shell(name))
}

/// Git's exec path is `<git>\mingw64\libexec\git-core`; its shells are in
/// `<git>\bin` and `<git>\usr\bin`.
fn git_shell(name: &str) -> Option<PathBuf> {
    let out = Command::new("git").arg("--exec-path").output().ok()?;
    let exec = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    let root = exec.ancestors().nth(3)?.to_path_buf();
    ["bin", "usr/bin"].iter().map(|d| root.join(d).join(format!("{name}.exe"))).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_command_says_what_to_write() {
        assert!(command("  ").unwrap_err().contains("empty"));
    }

    #[test]
    fn the_output_is_found_as_named() {
        let dir = std::env::temp_dir().join(format!("gmx-build-out-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        assert_eq!(output(&dir, "bin/tool"), None);
        let name = if cfg!(windows) { "bin/tool.exe" } else { "bin/tool" };
        std::fs::write(dir.join(name), b"x").unwrap();
        assert_eq!(output(&dir, "bin/tool"), Some(dir.join(name)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sh_build_runs_the_script() {
        let dir = std::env::temp_dir().join(format!("gmx-build-sh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("build"), "echo built > made\n").unwrap();
        let Ok(mut c) = command("sh build") else {
            eprintln!("skipping: no sh on this machine");
            return;
        };
        assert!(c.current_dir(&dir).status().unwrap().success());
        assert!(dir.join("made").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
