//! The environment a plugin process is given, read once into a struct.
//!
//! The table is 03 section 4. Everything else in the environment is inherited
//! from the core.

use std::path::PathBuf;

/// What the core told this process about itself, through the environment.
#[derive(Debug, Clone, Default)]
pub struct PluginEnv {
    /// `GMX_PLUGIN`: the plugin name, the namespace of every id.
    pub plugin: String,
    /// `GMX_PROVIDE`: the provide id this process serves.
    pub provide: String,
    /// `GMX_INSTANCE`: the instance id, a legible slug such as `cam1`.
    pub instance: String,
    /// `GMX_API_LEVEL`: the core's api level.
    pub api_level: u32,
    /// `GMX_PLUGIN_ROOT`: the absolute path of the plugin directory.
    pub root: PathBuf,
    /// `GMX_TOKEN`: the per instance token, scoped to this instance and its
    /// tools. Never log it.
    pub token: String,
    /// `GMX_RPC`: the WebSocket URL of the core's `/rpc`, for a plugin that
    /// calls core methods outside its stdio channel.
    pub rpc: String,
    /// `GMX_MEDIA`: the unixfd or shm address, after the handshake. Empty in
    /// container mode.
    pub media: String,
}

impl PluginEnv {
    /// Read the environment of this process.
    pub fn from_env() -> PluginEnv {
        PluginEnv::from_pairs(std::env::vars())
    }

    /// Read from any iterator of pairs, which is what the tests use.
    pub fn from_pairs<I: IntoIterator<Item = (String, String)>>(pairs: I) -> PluginEnv {
        let mut env = PluginEnv {
            root: PathBuf::from("."),
            api_level: 1,
            ..Default::default()
        };
        for (key, value) in pairs {
            match key.as_str() {
                "GMX_PLUGIN" => env.plugin = value,
                "GMX_PROVIDE" => env.provide = value,
                "GMX_INSTANCE" => env.instance = value,
                "GMX_API_LEVEL" => env.api_level = value.parse().unwrap_or(1),
                "GMX_PLUGIN_ROOT" => env.root = PathBuf::from(value),
                "GMX_TOKEN" => env.token = value,
                "GMX_RPC" => env.rpc = value,
                "GMX_MEDIA" => env.media = value,
                _ => {}
            }
        }
        env
    }

    /// Where crash reports go.
    pub fn crash_dir(&self) -> PathBuf {
        self.root.join("crashes")
    }

    /// Whether this process was started by a core at all. A plugin run by hand
    /// from a shell has none of these set, and the templates use this to print
    /// a help message instead of hanging on an empty stdin.
    pub fn started_by_core(&self) -> bool {
        !self.instance.is_empty()
    }

    /// A short line for a log: what this process is, with no token in it.
    pub fn describe(&self) -> String {
        format!(
            "{}/{} instance '{}' at api {}",
            if self.plugin.is_empty() {
                "?"
            } else {
                &self.plugin
            },
            if self.provide.is_empty() {
                "?"
            } else {
                &self.provide
            },
            if self.instance.is_empty() {
                "?"
            } else {
                &self.instance
            },
            self.api_level
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn every_documented_variable_is_read() {
        let env = PluginEnv::from_pairs(pairs(&[
            ("GMX_PLUGIN", "ndi"),
            ("GMX_PROVIDE", "source"),
            ("GMX_INSTANCE", "cam1"),
            ("GMX_API_LEVEL", "1"),
            ("GMX_PLUGIN_ROOT", "/opt/gmx/plugins/ndi/1.2.0"),
            ("GMX_TOKEN", "secret"),
            ("GMX_RPC", "ws://127.0.0.1:8080/rpc"),
            ("GMX_MEDIA", "/run/gmx/cam1.sock"),
            ("PATH", "/usr/bin"),
        ]));
        assert_eq!(env.plugin, "ndi");
        assert_eq!(env.instance, "cam1");
        assert_eq!(env.api_level, 1);
        assert_eq!(env.media, "/run/gmx/cam1.sock");
        assert_eq!(
            env.crash_dir(),
            PathBuf::from("/opt/gmx/plugins/ndi/1.2.0/crashes")
        );
        assert!(env.started_by_core());
    }

    #[test]
    fn an_empty_environment_is_usable() {
        let env = PluginEnv::from_pairs(Vec::new());
        assert_eq!(env.api_level, 1);
        assert_eq!(env.root, PathBuf::from("."));
        assert!(!env.started_by_core());
    }

    #[test]
    fn a_bad_api_level_falls_back_to_one() {
        let env = PluginEnv::from_pairs(pairs(&[("GMX_API_LEVEL", "not a number")]));
        assert_eq!(env.api_level, 1);
    }

    #[test]
    fn describe_never_carries_the_token() {
        let env = PluginEnv::from_pairs(pairs(&[
            ("GMX_PLUGIN", "ndi"),
            ("GMX_INSTANCE", "cam1"),
            ("GMX_TOKEN", "super-secret"),
        ]));
        assert!(!env.describe().contains("super-secret"));
    }
}
