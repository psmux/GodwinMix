//! `auto_source`: a stream that goes live becomes a mixer source, and goes
//! again when its publisher leaves, unless a scene holds it.
//!
//! The source is an `ingest/rtmp` that reads its stream from the listener
//! over loopback (`plugins/ingest/src/relay.rs`), named `<app>-<stream>`.
//! Taking it away is the mixer's ordinary `RemoveSource`, the same one
//! `source.remove` sends, so the programme carries on however it would for a
//! source an operator removed. A source a scene holds is left alone: it
//! waits, on its freeze frame, for the publisher to come back, and the
//! programme never sees it go.

use godwinmix_core::config::SourceConfig;
use godwinmix_core::mixer::{Command, RuntimeConfigs};
use serde_json::Value;
use tracing::{info, warn};

use super::keys::slug;
use super::{Channels, Live, Record};

/// The source type that reads a channel's stream.
const SOURCE_TYPE: &str = "ingest/rtmp";

impl Channels {
    fn configs(&self) -> Option<RuntimeConfigs> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.mixer.send(Command::Configs(tx)).ok()?;
        rx.blocking_recv().ok()
    }

    fn on_programme(&self) -> Option<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.mixer.send(Command::Status(tx)).ok()?;
        rx.blocking_recv().ok()?.program
    }

    /// Does anything still want this source: a scene that places it, or the
    /// programme showing it bare?
    pub(super) fn held(&self, source: &str) -> bool {
        if self.on_programme().as_deref() == Some(source) {
            return true;
        }
        let doc = self.scenes.document();
        let tree: Value = serde_json::from_str(&doc.to_json()).unwrap_or(Value::Null);
        places(&tree, source)
    }

    /// Make a live stream a source, or find the one it already has.
    pub(super) fn adopt(&self, record: &Record, stream: &str, relay: &str) {
        let id = slug(&format!("{}-{}", record.app, stream));
        let exists = self.configs().is_some_and(|c| c.sources.iter().any(|s| s.id == id));
        if !exists {
            let mut cfg = SourceConfig::bare(&id, "");
            cfg.type_id = Some(SOURCE_TYPE.into());
            cfg.name = Some(format!("{} {stream}", record.name));
            cfg.params.insert("relay".into(), toml::Value::String(relay.to_string()));
            cfg.params.insert("stream".into(), toml::Value::String(format!("{}/{stream}", record.app)));
            let (ack, told) = tokio::sync::oneshot::channel();
            let sent = self.mixer.send(Command::AddSource(Box::new(cfg), Some(ack)));
            match sent.ok().and_then(|_| told.blocking_recv().ok()) {
                Some(Ok(())) => info!(source = %id, "a channel's stream became a source"),
                other => {
                    warn!(source = %id, ?other, "the mixer would not take a channel's stream as a source");
                    return;
                }
            }
        }
        self.claim(record, stream, &id, !exists);
    }

    /// Write down that the stream feeds `id`, and that the channel owns it
    /// when it made it.
    fn claim(&self, record: &Record, stream: &str, id: &str, made: bool) {
        if let Some(l) = self.live.lock().iter_mut().find(|l| l.channel == record.id && l.name == stream) {
            l.source = Some(id.to_string());
        }
        if made {
            if let Some(r) = self.records.lock().iter_mut().find(|r| r.id == record.id) {
                if !r.auto_sources.iter().any(|s| s == id) {
                    r.auto_sources.push(id.to_string());
                }
            }
            if let Err(e) = self.persist() {
                warn!(error = %format!("{e:#}"), "could not save which sources a channel made");
            }
        }
    }

    /// Take away a source this module made, unless something holds it.
    /// Answers whether it went.
    pub(super) fn let_go(&self, id: &str) -> bool {
        let ours = self.records.lock().iter().any(|r| r.auto_sources.iter().any(|s| s == id));
        if !ours || self.held(id) {
            return false;
        }
        let (ack, told) = tokio::sync::oneshot::channel();
        if self.mixer.send(Command::RemoveSource(id.to_string(), Some(ack))).is_ok() {
            let _ = told.blocking_recv();
            info!(source = %id, "a channel's stream left and its source went with it");
        }
        for r in self.records.lock().iter_mut() {
            r.auto_sources.retain(|s| s != id);
        }
        if let Err(e) = self.persist() {
            warn!(error = %format!("{e:#}"), "could not save which sources a channel made");
        }
        true
    }

    /// At start: nothing is live yet, so each source a channel made either
    /// goes, or, when a scene holds it, waits as an idle stream.
    pub(super) fn tidy(&self) {
        let owned: Vec<(Record, String)> = self
            .records
            .lock()
            .iter()
            .flat_map(|r| r.auto_sources.iter().map(move |s| (r.clone(), s.clone())))
            .collect();
        for (record, source) in owned {
            if self.let_go(&source) {
                continue;
            }
            let stream = source.strip_prefix(&format!("{}-", slug(&record.app))).unwrap_or(&source);
            self.live.lock().push(Live {
                channel: record.id.clone(),
                app: record.app.clone(),
                name: stream.to_string(),
                state: "idle".into(),
                since_ms: 0,
                from: String::new(),
                key: None,
                video: None,
                audio: None,
                dropped_gops: 0,
                source: Some(source.clone()),
                relay: String::new(),
            });
        }
    }
}

/// Is there a `{"source": id}` anywhere in the scene tree?
fn places(tree: &Value, id: &str) -> bool {
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
