//! Where a channel's live streams become sources.
//!
//! In a core on its own that is its own mixer, and a source is held while
//! the programme shows it or a scene places it. Under a station the channels
//! live in the station and the mixer is in a show's process, so the station
//! gives [`Channels`](super::Channels) a target that reaches the show over its
//! socket instead (`crate::station::programme`). Every call here may block:
//! it is made from the channels' own thread or the blocking pool.

use godwinmix_core::config::SourceConfig;
use godwinmix_core::mixer::{Command, MixerHandle};
use godwinmix_core::scene::server::SceneServer;
use serde_json::Value;
use std::sync::Arc;

pub trait Programme: Send + Sync {
    /// Whether a source by this id exists. None when that cannot be told.
    fn has_source(&self, id: &str) -> Option<bool>;
    /// Whether the programme shows it or a scene places it.
    fn holds(&self, id: &str) -> bool;
    /// Add it, and say why not when it was refused.
    fn add_source(&self, cfg: SourceConfig) -> Result<(), String>;
    /// Take it away, as `source.remove` does.
    fn remove_source(&self, id: &str);
}

/// This process's own mixer and scenes.
pub struct Local {
    pub mixer: MixerHandle,
    pub scenes: Arc<SceneServer>,
}

impl Local {
    fn on_programme(&self) -> Option<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.mixer.send(Command::Status(tx)).ok()?;
        rx.blocking_recv().ok()?.program
    }
}

impl Programme for Local {
    fn has_source(&self, id: &str) -> Option<bool> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.mixer.send(Command::Configs(tx)).ok()?;
        rx.blocking_recv().ok().map(|c| c.sources.iter().any(|s| s.id == id))
    }

    fn holds(&self, id: &str) -> bool {
        if self.on_programme().as_deref() == Some(id) {
            return true;
        }
        let doc = self.scenes.document();
        let tree: Value = serde_json::from_str(&doc.to_json()).unwrap_or(Value::Null);
        places(&tree, id)
    }

    fn add_source(&self, cfg: SourceConfig) -> Result<(), String> {
        let (ack, told) = tokio::sync::oneshot::channel();
        let sent = self.mixer.send(Command::AddSource(Box::new(cfg), Some(ack)));
        match sent.ok().and_then(|_| told.blocking_recv().ok()) {
            Some(Ok(())) => Ok(()),
            Some(Err(e)) => Err(format!("{e:#}")),
            None => Err("the mixer is not running".into()),
        }
    }

    fn remove_source(&self, id: &str) {
        let (ack, told) = tokio::sync::oneshot::channel();
        if self.mixer.send(Command::RemoveSource(id.to_string(), Some(ack))).is_ok() {
            let _ = told.blocking_recv();
        }
    }
}

/// Is there a `{"source": id}` anywhere in the scene tree?
pub fn places(tree: &Value, id: &str) -> bool {
    match tree {
        Value::Object(map) => {
            map.get("source").and_then(Value::as_str) == Some(id) || map.values().any(|v| places(v, id))
        }
        Value::Array(items) => items.iter().any(|v| places(v, id)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_source_is_found_however_deep_a_scene_places_it() {
        let tree = json!({"scenes": [{"items": [{"children": [{"source": "church-main"}]}]}]});
        assert!(places(&tree, "church-main"));
        assert!(!places(&tree, "church-cam2"));
    }
}
