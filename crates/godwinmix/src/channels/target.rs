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
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tracing::warn;

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

/// How long a call waits for the mixer to answer, as long as a station gives
/// its show (`station::programme`). A mixer stuck inside one command costs
/// this thread five seconds, not the rest of its life.
const WAIT: Duration = Duration::from_secs(5);

/// A reply off the mixer's queue, waited for `WAIT` at most. A tokio oneshot
/// has no blocking wait with a deadline, so it is polled. The error says
/// which of the two it was, in the words `arrived_late` reads.
fn answer<T>(mut rx: oneshot::Receiver<T>, what: &str) -> Result<T, &'static str> {
    let deadline = Instant::now() + WAIT;
    loop {
        match rx.try_recv() {
            Ok(v) => return Ok(v),
            Err(oneshot::error::TryRecvError::Closed) => return Err("the mixer is not running"),
            Err(oneshot::error::TryRecvError::Empty) if Instant::now() >= deadline => {
                warn!(what, waited_ms = WAIT.as_millis() as u64, "the mixer did not answer a channel's call in time; going on without the answer");
                return Err("the mixer did not answer in time");
            }
            Err(oneshot::error::TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

impl Local {
    fn on_programme(&self) -> Option<String> {
        let (tx, rx) = oneshot::channel();
        self.mixer.send(Command::Status(tx)).ok()?;
        answer(rx, "core.status").ok()?.program
    }
}

impl Programme for Local {
    fn has_source(&self, id: &str) -> Option<bool> {
        let (tx, rx) = oneshot::channel();
        self.mixer.send(Command::Configs(tx)).ok()?;
        answer(rx, "source.list").ok().map(|c| c.sources.iter().any(|s| s.id == id))
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
        let (ack, told) = oneshot::channel();
        if let Err(e) = self.mixer.send(Command::AddSource(Box::new(cfg), Some(ack))) {
            return Err(format!("{e:#}"));
        }
        match answer(told, "source.add") {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(format!("{e:#}")),
            // "did not answer" makes `arrived_late` look again: a busy mixer
            // still adds it when it is free.
            Err(why) => Err(why.into()),
        }
    }

    fn remove_source(&self, id: &str) {
        let (ack, told) = oneshot::channel();
        if self.mixer.send(Command::RemoveSource(id.to_string(), Some(ack))).is_ok() {
            let _ = answer(told, "source.remove");
        }
    }
}

/// How often, and how far apart, to look for a source a busy show added late.
const LATE_TRIES: u32 = 4;
const LATE_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// After `add_source` said no: is the source there after all? Looked for
/// once for a refusal that says why, and a few times, a moment apart, for
/// one that only says the show did not answer in time, because a show that
/// was busy still adds it when it is free.
pub fn arrived_late(target: &dyn Programme, id: &str, why: &str) -> bool {
    let late = why.contains("did not answer") || why.contains("busy");
    for attempt in 0..if late { LATE_TRIES } else { 1 } {
        if attempt > 0 {
            std::thread::sleep(LATE_WAIT);
        }
        if target.has_source(id) == Some(true) {
            return true;
        }
    }
    false
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
