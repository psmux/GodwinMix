//! What `schemas/output.json` lets an operator change, and where the files go.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub directory: String,
    pub pattern: String,
    /// `mp4` or `mkv`.
    pub format: String,
    pub split_after_minutes: u32,
    pub min_free_bytes: u64,
    pub label: String,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            directory: String::new(),
            pattern: "{instance}-{datetime}".into(),
            format: "mp4".into(),
            split_after_minutes: 0,
            min_free_bytes: 1_000_000_000,
            label: String::new(),
        }
    }
}

impl Settings {
    pub fn from(params: &Value) -> Settings {
        let defaults = Settings::default();
        let format = text(params, "format");
        Settings {
            directory: text(params, "directory"),
            pattern: non_empty(params, "pattern").unwrap_or(defaults.pattern),
            format: if format == "mkv" {
                format
            } else {
                "mp4".into()
            },
            split_after_minutes: params
                .get("split_after_minutes")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(1440) as u32,
            min_free_bytes: params
                .get("min_free_gb")
                .and_then(Value::as_f64)
                .filter(|gb| *gb >= 0.0)
                .map(|gb| (gb * 1_000_000_000.0) as u64)
                .unwrap_or(defaults.min_free_bytes),
            label: text(params, "label"),
        }
    }

    /// Everything about a recording is fixed when the file is opened, so any
    /// change but the label needs the recording restarted. That is reported
    /// honestly as `restart_required` rather than pretended: cutting a service
    /// in half because somebody edited a path is worse than saying no.
    pub fn needs_restart(&self, next: &Settings) -> bool {
        self.directory != next.directory
            || self.pattern != next.pattern
            || self.format != next.format
            || self.split_after_minutes != next.split_after_minutes
    }

    /// The folder to write in, with the default filled in.
    pub fn folder(&self) -> PathBuf {
        if !self.directory.is_empty() {
            return PathBuf::from(&self.directory);
        }
        default_folder()
    }

    /// `0` means never.
    pub fn split_after(&self) -> Option<Duration> {
        (self.split_after_minutes > 0)
            .then(|| Duration::from_secs(self.split_after_minutes as u64 * 60))
    }

    pub fn extension(&self) -> &str {
        if self.format == "mkv" {
            "mkv"
        } else {
            "mp4"
        }
    }
}

/// A GodwinMix folder inside the operator's Videos folder, which is where a
/// person looks for a recording without being told.
pub fn default_folder() -> PathBuf {
    match std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        Some(home) => PathBuf::from(home).join("Videos").join("GodwinMix"),
        None => std::env::temp_dir().join("godwinmix-recordings"),
    }
}

fn text(params: &Value, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn non_empty(params: &Value, key: &str) -> Option<String> {
    Some(text(params, key)).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_object_is_every_default() {
        let s = Settings::from(&json!({}));
        assert_eq!(s, Settings::default());
        assert_eq!(s.extension(), "mp4");
        assert_eq!(s.min_free_bytes, 1_000_000_000);
        assert!(s.split_after().is_none());
    }

    #[test]
    fn an_unknown_format_falls_back_to_the_crash_safe_one() {
        assert_eq!(Settings::from(&json!({"format": "avi"})).extension(), "mp4");
        assert_eq!(Settings::from(&json!({"format": "mkv"})).extension(), "mkv");
    }

    #[test]
    fn minutes_become_a_duration_and_zero_means_one_file() {
        assert_eq!(
            Settings::from(&json!({"split_after_minutes": 30})).split_after(),
            Some(Duration::from_secs(1_800))
        );
        assert_eq!(
            Settings::from(&json!({"split_after_minutes": 0})).split_after(),
            None
        );
    }

    #[test]
    fn gigabytes_become_bytes() {
        assert_eq!(
            Settings::from(&json!({"min_free_gb": 5.0})).min_free_bytes,
            5_000_000_000
        );
        assert_eq!(
            Settings::from(&json!({"min_free_gb": 0.5})).min_free_bytes,
            500_000_000
        );
    }

    #[test]
    fn an_empty_folder_means_somewhere_a_person_will_look() {
        let folder = Settings::from(&json!({})).folder();
        assert!(folder.is_absolute(), "{folder:?}");
        assert!(folder.ends_with("GodwinMix") || folder.ends_with("godwinmix-recordings"));
    }

    #[test]
    fn only_the_label_can_change_without_restarting_the_recording() {
        let a = Settings::from(&json!({"directory": "/tmp/a"}));
        assert!(!a.needs_restart(&Settings::from(
            &json!({"directory": "/tmp/a", "label": "x"})
        )));
        assert!(a.needs_restart(&Settings::from(&json!({"directory": "/tmp/b"}))));
        assert!(a.needs_restart(&Settings::from(
            &json!({"directory": "/tmp/a", "format": "mkv"})
        )));
    }

    #[test]
    fn an_empty_pattern_does_not_produce_a_file_with_no_name() {
        assert_eq!(
            Settings::from(&json!({"pattern": ""})).pattern,
            "{instance}-{datetime}"
        );
    }
}
