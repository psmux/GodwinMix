//! Which Python a plugin's venv is made with, and the steps that make it.
//!
//! `python3` is the name on Linux and macOS. On Windows it is usually the
//! Microsoft Store's stand in, which prints a line about the Store and exits,
//! so a Python plugin installed there failed at its venv. Windows gets
//! `python` when a real one is on PATH and the `py -3` launcher otherwise.
//! `GMX_PYTHON` overrides all of it on every platform.
//!
//! When `uv` is on PATH it makes the venv, and then it has to install the
//! dependencies too: a venv made by `uv venv` has no pip in it, so
//! `.venv/bin/python -m pip` fails there.

use std::path::Path;

/// The interpreter to make a venv with, as a command and its first arguments.
pub fn interpreter() -> Vec<String> {
    if let Ok(set) = std::env::var("GMX_PYTHON") {
        if !set.trim().is_empty() {
            return vec![set];
        }
    }
    if cfg!(windows) {
        return windows_interpreter(&|name| crate::launch::which(name).map(|p| p.display().to_string()));
    }
    vec!["python3".into()]
}

/// `python` when a real one is on PATH, else `py -3`, else `python` so the
/// error names something a person recognises. `find` is `which`.
fn windows_interpreter(find: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let real = |path: &str| !path.to_ascii_lowercase().contains("windowsapps");
    if find("python").is_some_and(|p| real(&p)) {
        return vec!["python".into()];
    }
    if find("py").is_some() {
        return vec!["py".into(), "-3".into()];
    }
    vec!["python".into()]
}

/// The commands that make `.venv` in `root` and install what the plugin
/// lists. `uv` says whether uv is on PATH.
pub fn steps(root: &Path, uv: bool) -> Vec<Vec<String>> {
    let venv_python = crate::launch::venv_python(root).to_string_lossy().into_owned();
    let mut out = Vec::new();
    if uv {
        out.push(vec!["uv".into(), "venv".into(), ".venv".into()]);
    } else {
        let mut make = interpreter();
        make.extend(["-m".into(), "venv".into(), ".venv".into()]);
        out.push(make);
    }
    let what: Option<Vec<String>> = if root.join("pyproject.toml").exists() {
        Some(vec![".".into()])
    } else if root.join("requirements.txt").exists() {
        Some(vec!["-r".into(), "requirements.txt".into()])
    } else {
        None
    };
    if let Some(what) = what {
        let mut install: Vec<String> = if uv {
            vec!["uv".into(), "pip".into(), "install".into(), "--python".into(), venv_python]
        } else {
            vec![venv_python, "-m".into(), "pip".into(), "install".into()]
        };
        install.extend(what);
        out.push(install);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_skips_the_store_stand_in_and_falls_back_to_the_launcher() {
        let store = |name: &str| match name {
            "python" => Some(r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\python.exe".to_string()),
            "py" => Some(r"C:\Windows\py.exe".to_string()),
            _ => None,
        };
        assert_eq!(windows_interpreter(&store), ["py", "-3"]);
        let real = |name: &str| (name == "python").then(|| r"C:\Python314\python.exe".to_string());
        assert_eq!(windows_interpreter(&real), ["python"]);
        assert_eq!(windows_interpreter(&|_| None), ["python"]);
    }

    #[test]
    fn a_uv_venv_installs_its_requirements_with_uv_since_it_has_no_pip() {
        let dir = std::env::temp_dir().join(format!("gmx-python-steps-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("requirements.txt"), "requests\n").unwrap();
        let with_uv = steps(&dir, true);
        assert_eq!(with_uv[0], ["uv", "venv", ".venv"]);
        assert_eq!(&with_uv[1][..4], ["uv", "pip", "install", "--python"]);
        assert_eq!(&with_uv[1][5..], ["-r", "requirements.txt"]);
        let without = steps(&dir, false);
        assert!(without[0].ends_with(&["-m".to_string(), "venv".into(), ".venv".into()]), "{without:?}");
        assert_eq!(&without[1][1..], ["-m", "pip", "install", "-r", "requirements.txt"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_plugin_with_no_dependencies_only_gets_its_venv() {
        let none = std::env::temp_dir().join("gmx-python-steps-none-that-exists");
        assert_eq!(steps(&none, true).len(), 1);
    }
}
