//! What was on air, kept across a restart and put back at the next start.
//!
//! The core used to start on the first source in its list, whatever the
//! scenes said. A tester removed "Browser jaffer" from the only scene, closed
//! the app, opened it again and found Browser jaffer back in the header: the
//! source was still in the library, so the start put it on air by itself.
//!
//! Now every take is written down, a scene by name, a source by id, or black,
//! in a small file beside the runtime store, and the start puts that back.
//! With nothing written yet, or a scene or source that has gone since, it is
//! the first scene with something in it, and black when there is none.

use godwinmix_core::mixer::{Command, MixerHandle, ProgramScene};
use godwinmix_core::scene::server::SceneServer;
use godwinmix_protocol::types::Event;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// One take, as much of it as a restart needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnAir {
    Scene(String),
    Source(String),
    Black,
}

impl OnAir {
    /// What a `Took` event put on air. A one item scene names its source as
    /// well, and the scene is what a person took, so the scene wins.
    pub fn from_took(source: Option<&str>, scene: Option<&str>) -> OnAir {
        match (scene, source) {
            (Some(scene), _) => OnAir::Scene(scene.to_string()),
            (None, Some(source)) if !source.is_empty() => OnAir::Source(source.to_string()),
            _ => OnAir::Black,
        }
    }
}

/// `godwinmix.onair.json` beside `godwinmix.toml`.
pub fn path_for(config: &Path) -> PathBuf {
    let mut name = config.file_stem().unwrap_or_default().to_os_string();
    name.push(".onair.json");
    config.with_file_name(name)
}

pub fn read(path: &Path) -> Option<OnAir> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write then rename, so a crash mid write leaves the last good record.
pub fn write(path: &Path, on_air: &OnAir) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(on_air)?)?;
    std::fs::rename(&tmp, path)
}

/// Write down every take from now on. Off the mixer's thread: the events are
/// a broadcast, and a slow disk costs this task and nothing else.
pub fn spawn_recorder(handle: MixerHandle, path: PathBuf) {
    let mut events = handle.subscribe();
    tokio::spawn(async move {
        let mut last = None;
        loop {
            let envelope = match events.recv().await {
                Ok(envelope) => envelope,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            };
            let Event::Took { source, scene, .. } = envelope.event else { continue };
            let now = OnAir::from_took(source.as_deref(), scene.as_deref());
            if last.as_ref() == Some(&now) {
                continue;
            }
            let (path, record) = (path.clone(), now.clone());
            match tokio::task::spawn_blocking(move || write(&path, &record)).await {
                Ok(Ok(())) => debug!(on_air = ?now, "wrote down what is on air"),
                Ok(Err(e)) => warn!(error = %e, "could not write down what is on air; the next start may open on another scene"),
                Err(_) => {}
            }
            last = Some(now);
        }
    });
}

/// What the start should put on air, from the record and what exists now.
pub fn choose(record: Option<OnAir>, scenes: &SceneServer, sources: &[String]) -> OnAir {
    match record {
        Some(OnAir::Scene(name)) if scenes.scene(&name).is_ok() => return OnAir::Scene(name),
        Some(OnAir::Source(id)) if sources.contains(&id) => return OnAir::Source(id),
        Some(OnAir::Black) => return OnAir::Black,
        _ => {}
    }
    scenes
        .list()
        .into_iter()
        .find(|s| s.items > 0)
        .map(|s| OnAir::Scene(s.name))
        .unwrap_or(OnAir::Black)
}

/// Put back what was on air. Called once, after the scenes are open.
pub async fn restore(handle: &MixerHandle, scenes: &Arc<SceneServer>, path: &Path) {
    let sources = match handle.status().await {
        Ok(status) => status.sources.into_iter().map(|s| s.id).collect::<Vec<_>>(),
        Err(_) => return,
    };
    let chosen = choose(read(path), scenes, &sources);
    info!(on_air = ?chosen, "putting back what was on air");
    let sent = match &chosen {
        OnAir::Scene(name) => match scenes.placements(name) {
            Ok((name, placements)) => handle.send(Command::TakeScene {
                scene: Box::new(ProgramScene { name, placements }),
                at_running_time_ms: None,
                duration_ms: None,
                transition: None,
                ack: None,
            }),
            Err(_) => return,
        },
        OnAir::Source(id) => handle.send(Command::Take { source: Some(id.clone()), at_running_time_ms: None, ack: None }),
        OnAir::Black => return,
    };
    if let Err(e) = sent {
        warn!(error = %e, "could not put back what was on air");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_take_is_written_as_what_a_person_took() {
        assert_eq!(OnAir::from_took(Some("cam1"), Some("Interview")), OnAir::Scene("Interview".into()));
        assert_eq!(OnAir::from_took(Some("cam1"), None), OnAir::Source("cam1".into()));
        assert_eq!(OnAir::from_took(None, None), OnAir::Black);
        assert_eq!(OnAir::from_took(Some(""), None), OnAir::Black);
    }

    #[test]
    fn the_record_survives_a_round_trip_through_its_file() {
        let dir = std::env::temp_dir().join(format!("gmx-onair-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = path_for(&dir.join("godwinmix.toml"));
        assert!(path.ends_with("godwinmix.onair.json"));
        for on_air in [OnAir::Scene("Default scene".into()), OnAir::Source("cam1".into()), OnAir::Black] {
            write(&path, &on_air).unwrap();
            assert_eq!(read(&path), Some(on_air));
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
