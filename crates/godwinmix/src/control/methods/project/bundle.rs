//! The project file: one JSON document, and the checks it has to pass before
//! anything in it is believed.
//!
//! JSON rather than a zip, for three reasons. The page reads and writes it with
//! `JSON.parse` and a `Blob` and no library, which matters on a page with a
//! byte budget. It travels over JSON-RPC as itself, where a zip would be
//! base64 inside JSON. And a person can open it and see what is in it, which
//! is what they want from a file that may carry stream keys. Clips, when they
//! are asked for, ride inside as base64; a big media folder is better copied
//! as a folder, and the export says so.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

use godwinmix_protocol::error::RpcError;

/// What `format` says in every project file.
pub const FORMAT: &str = "godwinmix.project";
/// The newest project format this core reads and the one it writes.
pub const VERSION: u32 = 1;

/// One project: everything a show is, and nothing that belongs to the machine
/// it happens to run on unless it is asked for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    /// `godwinmix 0.2.0`, for a person and for the refusal a newer file gets.
    #[serde(default)]
    pub written_by: String,
    /// Milliseconds since 1970, when it was exported.
    #[serde(default)]
    pub exported_at: u64,
    /// What the person calls it. Becomes the show's name when shows arrive.
    #[serde(default)]
    pub name: String,
    /// True when stream keys, channel keys and the control token are inside.
    #[serde(default)]
    pub secrets: bool,
    /// Show settings by dotted key: canvas, programme, multiview, safety.
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    /// Settings of the machine (addresses, folders, hardware). Written back
    /// only when an import asks for them.
    #[serde(default)]
    pub machine: BTreeMap<String, Value>,
    #[serde(default)]
    pub sources: Vec<Value>,
    /// Destinations, each with its rendition when it asked for one.
    #[serde(default)]
    pub outputs: Vec<Value>,
    #[serde(default)]
    pub channels: Vec<Value>,
    /// The scene collection, as `scene.export` with `format: "json"` gives it.
    #[serde(default)]
    pub scenes: Value,
    /// Whatever the page sent along: its layout and its settings.
    #[serde(default)]
    pub page: Value,
    #[serde(default)]
    pub media: Vec<MediaEntry>,
    /// What was taken out, in words: "output youtube: its stream key".
    #[serde(default)]
    pub removed: Vec<String>,
}

/// A clip in the media folder, by name and size, and its bytes when asked.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MediaEntry {
    pub name: String,
    pub size: u64,
    /// Base64, present only when the export was asked to include media.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl Bundle {
    pub fn new(name: &str, secrets: bool) -> Self {
        let exported_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Bundle {
            format: FORMAT.into(),
            version: VERSION,
            written_by: format!("godwinmix {}", env!("CARGO_PKG_VERSION")),
            exported_at,
            name: name.to_string(),
            secrets,
            settings: BTreeMap::new(),
            machine: BTreeMap::new(),
            sources: Vec::new(),
            outputs: Vec::new(),
            channels: Vec::new(),
            scenes: Value::Null,
            page: Value::Null,
            media: Vec::new(),
            removed: Vec::new(),
        }
    }
}

/// Read a file as sent: the object itself, or its text.
///
/// Every refusal names what is wrong with the file and what to do instead,
/// because the person holding it cannot see inside it.
pub fn read(file: &Value) -> Result<Bundle, RpcError> {
    let value = match file {
        Value::String(text) => serde_json::from_str::<Value>(text).map_err(|e| {
            refuse(format!("this file is not JSON, so it is not a GodwinMix project ({e}). Choose a .gmxproject file saved by Save project as."), "not_json")
        })?,
        other => other.clone(),
    };
    let Some(object) = value.as_object() else {
        return Err(refuse("this file holds no project: it is not a JSON object. Choose a .gmxproject file saved by Save project as.".into(), "not_object"));
    };
    check_format(object)?;
    check_version(object)?;
    serde_json::from_value(value.clone()).map_err(|e| {
        refuse(format!("this project file is damaged: {e}. Export it again from the mixer it came from."), "damaged")
    })
}

fn check_format(object: &Map<String, Value>) -> Result<(), RpcError> {
    match object.get("format").and_then(Value::as_str) {
        Some(FORMAT) => Ok(()),
        _ if object.contains_key("schemaVersion") && object.contains_key("scenes") => Err(refuse(
            "this is a scene collection, not a project. Import it from the Scenes panel, which adds its scenes to this mixer.".into(),
            "scene_collection",
        )),
        Some(other) => Err(refuse(format!("this file says it is a {other:?}, not a GodwinMix project. Choose a .gmxproject file saved by Save project as."), "wrong_format")),
        None => Err(refuse("this file has no \"format\": \"godwinmix.project\" line, so it is not a GodwinMix project. Choose a .gmxproject file saved by Save project as.".into(), "wrong_format")),
    }
}

fn check_version(object: &Map<String, Value>) -> Result<(), RpcError> {
    let version = object.get("version").and_then(Value::as_u64).unwrap_or(0);
    let by = object.get("written_by").and_then(Value::as_str).unwrap_or("a GodwinMix of unknown version").to_string();
    if version == 0 {
        return Err(refuse("this project file has no version number, so it cannot be read safely. Export it again from the mixer it came from.".into(), "no_version"));
    }
    if version > VERSION as u64 {
        return Err(refuse(
            format!(
                "this project was saved by {by} in project format {version}, and this mixer (godwinmix {}) reads format {VERSION} and older. Update this mixer, then open the file again.",
                env!("CARGO_PKG_VERSION")
            ),
            "newer_version",
        )
        .with("version", version)
        .with("supported", VERSION)
        .with("written_by", by));
    }
    Ok(())
}

fn refuse(message: String, reason: &str) -> RpcError {
    RpcError::invalid_params(message).with("field", "file").with("reason", reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_file_from_a_newer_version_is_refused_with_the_reason() {
        let file = json!({"format": FORMAT, "version": 7, "written_by": "godwinmix 9.1.0"});
        let e = read(&file).unwrap_err();
        let text = e.message.clone();
        assert!(text.contains("format 7") && text.contains("godwinmix 9.1.0") && text.contains("Update this mixer"), "{text}");
        assert_eq!(e.data["reason"], "newer_version");
        assert_eq!(e.data["supported"], VERSION);
        assert_eq!(e.data["field"], "file");
    }

    #[test]
    fn a_scene_collection_and_plain_text_are_named_for_what_they_are() {
        let e = read(&json!({"schemaVersion": 1, "scenes": []})).unwrap_err();
        assert!(e.message.contains("scene collection"), "{}", e.message);
        let e = read(&Value::String("not json".into())).unwrap_err();
        assert!(e.message.contains("not JSON"), "{}", e.message);
        let e = read(&json!({"format": FORMAT})).unwrap_err();
        assert!(e.message.contains("no version"), "{}", e.message);
    }

    #[test]
    fn a_file_written_here_reads_back_from_its_text() {
        let written = serde_json::to_value(Bundle::new("Sunday", false)).unwrap();
        let text = serde_json::to_string(&written).unwrap();
        let read_back = read(&Value::String(text)).unwrap();
        assert_eq!(read_back.name, "Sunday");
        assert_eq!(read_back.version, VERSION);
    }
}
