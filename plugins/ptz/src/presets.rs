//! Presets as pictures: the camera keeps where preset n points, and this
//! keeps a name and a thumbnail taken when it was saved, so a panel shows a
//! grid of shots rather than a row of numbers.
//!
//! On disk, per camera: `<data_dir>/<camera>/presets.json` with each preset's
//! name and time, and `preset-<n>.jpg` beside it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub number: u16,
    pub name: String,
    /// Seconds since 1970.
    pub saved_at: u64,
    pub thumbnail: Option<PathBuf>,
}

impl Preset {
    /// The record a caller sees. `inline` adds the thumbnail as a data URL.
    pub fn public(&self, inline: bool) -> Value {
        let mut v = json!({"preset": self.number, "name": self.name, "saved_at": self.saved_at, "thumbnail": self.thumbnail});
        if inline {
            if let Some(bytes) = self.thumbnail.as_ref().and_then(|p| std::fs::read(p).ok()) {
                v["thumbnail_data"] = json!(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
            }
        }
        v
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(data_dir: &Path, camera: &str) -> Store {
        Store { dir: data_dir.join(camera) }
    }

    fn index(&self) -> PathBuf {
        self.dir.join("presets.json")
    }

    pub fn list(&self) -> Vec<Preset> {
        let text = std::fs::read_to_string(self.index()).unwrap_or_default();
        let map: Map<String, Value> = serde_json::from_str(&text).unwrap_or_default();
        let mut out: BTreeMap<u16, Preset> = BTreeMap::new();
        for (k, v) in map {
            let Ok(number) = k.parse::<u16>() else { continue };
            let jpg = self.dir.join(format!("preset-{number}.jpg"));
            out.insert(number, Preset {
                number,
                name: v.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                saved_at: v.get("saved_at").and_then(Value::as_u64).unwrap_or(0),
                thumbnail: jpg.exists().then_some(jpg),
            });
        }
        out.into_values().collect()
    }

    pub fn get(&self, number: u16) -> Option<Preset> {
        self.list().into_iter().find(|p| p.number == number)
    }

    /// Record preset `number`, with the picture when there is one. A preset
    /// saved again replaces its name and picture.
    pub fn save(&self, number: u16, name: &str, jpeg: Option<&[u8]>) -> std::io::Result<Preset> {
        std::fs::create_dir_all(&self.dir)?;
        let jpg = self.dir.join(format!("preset-{number}.jpg"));
        match jpeg {
            Some(bytes) => std::fs::write(&jpg, bytes)?,
            None => {
                let _ = std::fs::remove_file(&jpg);
            }
        }
        let text = std::fs::read_to_string(self.index()).unwrap_or_default();
        let mut map: Map<String, Value> = serde_json::from_str(&text).unwrap_or_default();
        let saved_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        map.insert(number.to_string(), json!({"name": name, "saved_at": saved_at}));
        let tmp = self.dir.join("presets.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&map).unwrap_or_default())?;
        std::fs::rename(&tmp, self.index())?;
        Ok(Preset { number, name: name.to_string(), saved_at, thumbnail: jpeg.map(|_| jpg) })
    }
}
