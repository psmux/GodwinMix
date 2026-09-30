//! Where a channel's live stream becomes a source under a station: the first
//! show, reached over its socket with the public methods, so a single show
//! setup behaves as it did in one process. Another show adds a channel's
//! stream by hand, from the relay address `channel.list` gives, the same as
//! any source.
//!
//! Called from the channels' own thread and the blocking pool, never from
//! the runtime, so waiting on the runtime here is allowed.

use super::state::Station;
use crate::channels::target::{places, Programme};
use godwinmix_core::config::SourceConfig;
use serde_json::{json, Value};
use std::sync::Weak;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(5);

pub struct FirstShow {
    pub station: Weak<Station>,
    pub runtime: tokio::runtime::Handle,
}

impl FirstShow {
    fn ask(&self, method: &str, params: Value) -> Result<Value, String> {
        let st = self.station.upgrade().ok_or("the station is stopping")?;
        let id = st.first();
        self.runtime.block_on(st.ask_show(&id, method, params, WAIT)).map_err(|e| e.message)
    }
}

impl Programme for FirstShow {
    fn has_source(&self, id: &str) -> Option<bool> {
        let list = self.ask("source.list", json!({})).ok()?;
        let rows = list.get("sources").and_then(Value::as_array).or_else(|| list.as_array())?.clone();
        Some(rows.iter().any(|s| s.get("id").and_then(Value::as_str) == Some(id)))
    }

    fn holds(&self, id: &str) -> bool {
        let on_air = self.ask("core.status", json!({})).ok();
        if on_air.as_ref().and_then(|s| s.get("program")).and_then(Value::as_str) == Some(id) {
            return true;
        }
        self.ask("scene.list", json!({})).map(|doc| places(&doc, id)).unwrap_or(true)
    }

    fn add_source(&self, cfg: SourceConfig) -> Result<(), String> {
        let mut params = serde_json::Map::new();
        for (k, v) in &cfg.params {
            params.insert(k.clone(), serde_json::to_value(v).unwrap_or_default());
        }
        let stream = params.get("stream").and_then(Value::as_str).unwrap_or_default().to_string();
        params.insert("id".into(), json!(cfg.id));
        params.insert("name".into(), json!(cfg.name));
        params.insert("type".into(), json!(cfg.type_id));
        params.insert("uri".into(), json!(format!("channel:{stream}")));
        self.ask("source.add", Value::Object(params)).map(|_| ())
    }

    fn remove_source(&self, id: &str) {
        let _ = self.ask("source.remove", json!({ "id": id }));
    }
}
