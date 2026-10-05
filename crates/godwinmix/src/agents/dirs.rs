//! The folders a setup's paths are worked out from.

use std::path::PathBuf;

/// The folders the paths are worked out from. Read from the environment in
/// use, and made up in a test so nothing in a real home is touched.
#[derive(Debug, Clone)]
pub struct Dirs {
    pub home: PathBuf,
    /// Where VS Code keeps its user folder: `%APPDATA%`, `~/Library/Application
    /// Support` or `$XDG_CONFIG_HOME`.
    pub app_config: PathBuf,
}

impl Dirs {
    pub fn from_env() -> Option<Dirs> {
        let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = env("HOME").or_else(|| env("USERPROFILE"))?;
        let app_config = if cfg!(windows) {
            env("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming"))
        } else if cfg!(target_os = "macos") {
            home.join("Library").join("Application Support")
        } else {
            env("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"))
        };
        Some(Dirs { home, app_config })
    }
}
