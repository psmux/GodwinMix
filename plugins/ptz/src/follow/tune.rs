//! How a camera follows, in numbers a person or an agent can set while it
//! runs: `track_tune`. Kept per camera, beside its presets, so it outlasts
//! a restart.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use godwinmix_track::{Fallback, FollowConfig};
use serde_json::{json, Value};

/// Each field with its range and what it does, for errors and the schema.
pub const FIELDS: &[(&str, f32, f32, &str)] = &[
    ("headroom", 0.0, 0.4, "space above the head, a fraction of the shot height"),
    ("lead_room", 0.0, 2.0, "how far ahead of a walking person to aim, in seconds of their walk"),
    ("dead_zone", 0.0, 0.3, "how far off centre the person may drift before the camera moves"),
    ("max_speed", 0.05, 1.0, "the fastest the camera turns or zooms, 1 being its top speed"),
    ("max_accel", 0.2, 10.0, "how fast the speed may change each second; lower is smoother"),
    ("lost_hold_ms", 0.0, 30000.0, "how long to hold the shot when the person vanishes"),
    ("switch_margin", 1.0, 5.0, "how much better someone else must score before the camera switches to them"),
    ("detect_every", 1.0, 10.0, "look for people on one picture in this many, tracking in between"),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Tuning {
    pub values: [f32; 8],
    /// `stay`, `wide` or `preset:<n>`: what to do once the hold runs out.
    pub fallback: String,
}

impl Default for Tuning {
    fn default() -> Self {
        let c = FollowConfig::default();
        Tuning {
            values: [
                c.style.headroom,
                c.style.lead_room,
                c.ptz.dead_zone,
                c.ptz.max_speed,
                c.ptz.max_accel,
                c.choose.lost_hold.as_millis() as f32,
                c.choose.switch_margin,
                c.detect_every as f32,
            ],
            fallback: "stay".into(),
        }
    }
}

impl Tuning {
    fn get(&self, name: &str) -> f32 {
        self.values[FIELDS.iter().position(|f| f.0 == name).expect("a known field")]
    }

    /// Take what `a` sets, checking every value first so a bad one changes
    /// nothing. Answers with the names it changed.
    pub fn merge(&mut self, a: &Value) -> Result<Vec<String>, String> {
        let mut next = self.clone();
        let mut changed = Vec::new();
        for (i, (name, lo, hi, what)) in FIELDS.iter().enumerate() {
            let Some(v) = a.get(*name).filter(|v| !v.is_null()) else { continue };
            let n = v.as_f64().ok_or_else(|| format!("`{name}` is a number: {what}."))? as f32;
            if !(*lo..=*hi).contains(&n) {
                return Err(format!("`{name}` {n} is outside {lo} to {hi}. It is {what}."));
            }
            next.values[i] = n;
            changed.push(name.to_string());
        }
        if let Some(f) = a.get("fallback").and_then(Value::as_str) {
            let ok = f == "stay" || f == "wide" || f.strip_prefix("preset:").is_some_and(|n| n.parse::<u16>().is_ok());
            if !ok {
                return Err(format!("`fallback` '{f}' is not stay, wide or preset:<number>, such as preset:1."));
            }
            next.fallback = f.to_string();
            changed.push("fallback".into());
        }
        *self = next;
        Ok(changed)
    }

    /// Into a running follower's settings.
    pub fn apply(&self, c: &mut FollowConfig) {
        c.style.headroom = self.get("headroom");
        c.style.lead_room = self.get("lead_room");
        c.ptz.dead_zone = self.get("dead_zone");
        c.ptz.max_speed = self.get("max_speed");
        c.ptz.max_accel = self.get("max_accel");
        c.choose.lost_hold = Duration::from_millis(self.get("lost_hold_ms") as u64);
        c.choose.switch_margin = self.get("switch_margin");
        c.detect_every = self.get("detect_every").round() as u32;
        c.choose.fallback = match self.fallback.as_str() {
            "wide" => Fallback::Wide,
            f => f.strip_prefix("preset:").map_or(Fallback::Stay, |n| Fallback::Preset(Arc::from(n))),
        };
    }

    pub fn to_json(&self) -> Value {
        let mut v = json!({"fallback": self.fallback});
        for (i, (name, ..)) in FIELDS.iter().enumerate() {
            let x = self.values[i];
            v[*name] = if *name == "lost_hold_ms" || *name == "detect_every" { json!(x.round() as u64) } else { json!((f64::from(x) * 1000.0).round() / 1000.0) };
        }
        v
    }

    fn file(data_dir: &Path, camera: &str) -> PathBuf {
        data_dir.join(camera).join("tuning.json")
    }

    /// This camera's saved tuning, or the defaults.
    pub fn load(data_dir: &Path, camera: &str) -> Tuning {
        let mut t = Tuning::default();
        if let Some(v) = std::fs::read_to_string(Self::file(data_dir, camera)).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
            let _ = t.merge(&v);
        }
        t
    }

    pub fn save(&self, data_dir: &Path, camera: &str) -> Result<(), String> {
        let path = Self::file(data_dir, camera);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("making {}: {e}", dir.display()))?;
        }
        std::fs::write(&path, self.to_json().to_string()).map_err(|e| format!("writing {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_value_changes_nothing_and_says_the_range() {
        let mut t = Tuning::default();
        let err = t.merge(&json!({"headroom": 0.2, "max_speed": 3.0})).unwrap_err();
        assert!(err.contains("max_speed") && err.contains("0.05 to 1"), "{err}");
        assert_eq!(t, Tuning::default(), "nothing was taken");
        assert_eq!(t.merge(&json!({"headroom": 0.2, "fallback": "preset:2"})).unwrap(), vec!["headroom", "fallback"]);
        let mut c = FollowConfig::default();
        t.apply(&mut c);
        assert_eq!((c.style.headroom, c.choose.fallback), (0.2, Fallback::Preset(Arc::from("2"))));
    }

    #[test]
    fn it_is_kept_beside_the_presets() {
        let dir = std::env::temp_dir().join(format!("gmx-ptz-tune-{}", std::process::id()));
        let mut t = Tuning::default();
        t.merge(&json!({"max_speed": 0.3, "lost_hold_ms": 4000})).unwrap();
        t.save(&dir, "cam-pulpit").unwrap();
        assert_eq!(Tuning::load(&dir, "cam-pulpit"), t);
        let _ = std::fs::remove_dir_all(dir);
    }
}
