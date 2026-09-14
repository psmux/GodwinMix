//! `list_recordings`, the one tool this plugin contributes.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use godwinmix_capture_common::space;
use godwinmix_sdk::prelude::*;
use godwinmix_sdk::wire::{ToolCall, ToolResult};
use serde_json::{json, Value};

use crate::settings::Settings;

/// At most this many files come back. A folder with a year of services in it
/// should not put a megabyte on the control channel.
const MOST: usize = 50;

pub fn dispatch(
    method: &str,
    params: Value,
    settings: &Settings,
    current: Option<&str>,
) -> Result<Value, RpcError> {
    if method != "tool.call" {
        return Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!(
                "this plugin has no method '{method}'. It answers the output methods and \
                 `tool.call` with name 'list_recordings'."
            ),
        )
        .with_data(json!({"method": method, "retryable": false})));
    }
    let call: ToolCall = serde_json::from_value(params).map_err(|e| {
        RpcError::new(
            codes::INVALID_PARAMS,
            format!("`tool.call` needs {{name, arguments}}: {e}"),
        )
    })?;
    if call.name != "list_recordings" {
        return Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!(
                "this plugin has no tool '{}'. It has one: 'list_recordings'.",
                call.name
            ),
        ));
    }
    let answer = listing(settings, current);
    let result = ToolResult {
        content: json!([{"type": "text", "text": summary(&answer)}]),
        structured_content: Some(answer),
        is_error: Some(false),
    };
    serde_json::to_value(result)
        .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, format!("could not encode: {e}")))
}

/// What is in the folder, newest first.
pub fn listing(settings: &Settings, current: Option<&str>) -> Value {
    let folder = settings.folder();
    let mut answer = json!({
        "directory": folder.display().to_string(),
        "recordings": files(&folder, current),
    });
    if let Some(free) = space::free_bytes(&folder) {
        answer["free_bytes"] = json!(free);
    }
    answer
}

fn files(folder: &Path, current: Option<&str>) -> Vec<Value> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<(SystemTime, Value)> = entries
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
            let value = json!({
                "name": name,
                "bytes": metadata.len(),
                "modified": rfc3339(modified),
                "recording": current.is_some_and(|c| c.ends_with(&name)),
            });
            Some((modified, value))
        })
        .collect();
    // Newest first, which is the order an operator wants after a service.
    found.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    found.into_iter().take(MOST).map(|(_, v)| v).collect()
}

/// A timestamp as RFC 3339 in UTC, built from the seconds since the epoch.
///
/// `glib` is already here behind GStreamer and does the calendar arithmetic,
/// so this is not another date crate.
fn rfc3339(at: SystemTime) -> String {
    let seconds = at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    gstreamer::glib::DateTime::from_unix_utc(seconds as i64)
        .ok()
        .and_then(|t| t.format("%Y-%m-%dT%H:%M:%SZ").ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn summary(answer: &Value) -> String {
    let folder = answer["directory"].as_str().unwrap_or("");
    let count = answer["recordings"].as_array().map(Vec::len).unwrap_or(0);
    let free = answer["free_bytes"]
        .as_u64()
        .map(|b| format!(", {} free", space::human(b)))
        .unwrap_or_default();
    if count == 0 {
        return format!("nothing in {folder} yet{free}");
    }
    format!("{count} recording(s) in {folder}{free}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json as j;

    fn settings_in(dir: &Path) -> Settings {
        Settings::from(&j!({"directory": dir.display().to_string()}))
    }

    #[test]
    fn an_unknown_method_names_the_one_it_has() {
        let err = dispatch("teleport", j!({}), &Settings::default(), None).expect_err("no such");
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
        assert!(err.message.contains("list_recordings"), "{}", err.message);
    }

    #[test]
    fn a_folder_that_is_not_there_yet_lists_nothing_rather_than_failing() {
        let settings = Settings::from(&j!({"directory": "/no/such/folder"}));
        let answer = listing(&settings, None);
        assert_eq!(answer["recordings"].as_array().unwrap().len(), 0);
        assert!(summary(&answer).contains("nothing in"));
    }

    #[test]
    fn files_come_back_newest_first_with_the_live_one_marked() {
        let dir = std::env::temp_dir().join(format!("gmx-list-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old.mp4"), vec![0u8; 10]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1_100));
        std::fs::write(dir.join("new.mp4"), vec![0u8; 20]).unwrap();

        let live = dir.join("new.mp4").display().to_string();
        let answer = listing(&settings_in(&dir), Some(&live));
        let list = answer["recordings"].as_array().unwrap();
        assert_eq!(list[0]["name"], "new.mp4", "newest first");
        assert_eq!(list[0]["bytes"], 20);
        assert_eq!(list[0]["recording"], true);
        assert_eq!(list[1]["recording"], false);
        assert!(list[0]["modified"].as_str().unwrap().contains('T'));
        assert!(answer["free_bytes"].is_u64(), "the disk reading is there");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tool_answers_through_dispatch() {
        let value = dispatch(
            "tool.call",
            j!({"name": "list_recordings", "arguments": {}}),
            &Settings::default(),
            None,
        )
        .expect("listing never fails");
        assert!(value["structured_content"]["directory"].is_string());
    }

    #[test]
    fn an_unknown_tool_names_the_one_it_has() {
        let err = dispatch(
            "tool.call",
            j!({"name": "delete_everything", "arguments": {}}),
            &Settings::default(),
            None,
        )
        .expect_err("no such tool");
        assert!(err.message.contains("list_recordings"), "{}", err.message);
    }
}
