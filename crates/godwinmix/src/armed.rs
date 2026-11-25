//! What was armed in Preview, and what After Take was set to, kept across a
//! restart.
//!
//! A tester restarted the mixer with Scene 2 armed. The core came back with
//! nothing armed, so `program.take {}` (the Take on a Stream Deck) had nothing
//! to take, and a page left open still named Scene 2 over an empty picture.
//! `on_air.rs` already writes down every take; this writes down every arm
//! beside it, in `godwinmix.armed.json`, and the start arms it again.
//!
//! With nothing written yet (a core older than this file), the start arms
//! the scene it put back on air when After Take is "same", because that is
//! what Preview holds after any take under that setting.

use crate::on_air::OnAir;
use godwinmix_core::mixer::MixerHandle;
use godwinmix_core::scene::server::{AfterTake, SceneServer};
use godwinmix_protocol::types::Event;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// The armed scene by id, so a rename does not lose it, and the mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Armed {
    pub scene: Option<String>,
    #[serde(default)]
    pub after_take: AfterTake,
}

impl Armed {
    pub fn now(scenes: &SceneServer) -> Armed {
        Armed { scene: scenes.armed().map(|id| id.to_string()), after_take: scenes.after_take() }
    }
}

/// `godwinmix.armed.json` beside `godwinmix.toml`.
pub fn path_for(config: &Path) -> PathBuf {
    let mut name = config.file_stem().unwrap_or_default().to_os_string();
    name.push(".armed.json");
    config.with_file_name(name)
}

pub fn read(path: &Path) -> Option<Armed> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Write then rename, as `on_air::write` does.
pub fn write(path: &Path, armed: &Armed) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(armed)?)?;
    std::fs::rename(&tmp, path)
}

/// Write down every arm from now on. `preview.changed` is raised by an arm,
/// by a take that moved what is armed, and by a change of After Take, so it
/// is the one event to follow. Off the mixer's thread, as the on air record.
pub fn spawn_recorder(handle: MixerHandle, scenes: Arc<SceneServer>, path: PathBuf) {
    let mut events = handle.subscribe();
    tokio::spawn(async move {
        let mut last = None;
        loop {
            match events.recv().await {
                Ok(envelope) if matches!(envelope.event, Event::PreviewChanged { .. }) => {}
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            }
            let now = Armed::now(&scenes);
            if last.as_ref() == Some(&now) {
                continue;
            }
            let (path, record) = (path.clone(), now.clone());
            match tokio::task::spawn_blocking(move || write(&path, &record)).await {
                Ok(Ok(())) => debug!(armed = ?now, "wrote down what is armed"),
                Ok(Err(e)) => warn!(error = %e, "could not write down what is armed; the next start may arm another scene"),
                Err(_) => {}
            }
            last = Some(now);
        }
    });
}

/// Arm again what was armed, after `on_air::restore` put back `on_air`.
/// Called once at the start, before any client can connect, so nobody needs
/// telling: every client hears the armed scene when it subscribes.
pub fn restore(scenes: &SceneServer, record: Option<Armed>, on_air: &OnAir) {
    let Some(record) = record else {
        if let OnAir::Scene(name) = on_air {
            if scenes.after_take() == AfterTake::Same && scenes.arm(Some(name)).is_ok() {
                info!(scene = %name, "armed the scene on air, with nothing written down from before");
            }
        }
        return;
    };
    scenes.set_after_take(record.after_take);
    let Some(scene) = record.scene else { return };
    match scenes.arm(Some(&scene)) {
        Ok(Some(armed)) => info!(scene = %armed.name, "armed again what was armed"),
        Ok(None) => {}
        // Deleted since. Preview follows what is on air instead, as it would
        // have after a take.
        Err(_) => {
            if let OnAir::Scene(name) = on_air {
                scenes.arm(Some(name)).ok();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_core::caps::CanvasCaps;
    use godwinmix_core::scene::document::{Content, Item, Scene};

    fn server() -> Arc<SceneServer> {
        let s = SceneServer::in_memory(CanvasCaps::new(&godwinmix_core::config::Canvas {
            width: 1920,
            height: 1080,
            fps: godwinmix_core::config::FrameRate::whole(30),
            sample_rate: 48000,
            channels: 2,
        }));
        s.edit(None, |doc| {
            for name in ["Scene 1", "Scene 2"] {
                let mut scene = Scene::new(name);
                scene.items.push(Item::new(Content::Source { source: "cam1".into() }));
                doc.scenes.push(scene);
            }
            Ok(())
        })
        .unwrap();
        s
    }

    fn armed_name(s: &SceneServer) -> Option<String> {
        s.armed().and_then(|id| s.scene(&id.to_string()).ok()).map(|v| v.name)
    }

    #[test]
    fn a_restart_arms_again_what_was_armed_and_keeps_after_take() {
        let before = server();
        before.arm(Some("Scene 2")).unwrap();
        before.set_after_take(AfterTake::Swap);
        let dir = std::env::temp_dir().join(format!("gmx-armed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = path_for(&dir.join("godwinmix.toml"));
        assert!(path.ends_with("godwinmix.armed.json"));
        write(&path, &Armed::now(&before)).unwrap();

        // The same collection opened again: ids survive, the armed scene did not.
        let after = server();
        after.edit(None, |doc| {
            *doc = before.document();
            Ok(())
        })
        .unwrap();
        assert_eq!(after.armed(), None);
        restore(&after, read(&path), &OnAir::Scene("Scene 1".into()));
        assert_eq!(armed_name(&after).as_deref(), Some("Scene 2"));
        assert_eq!(after.after_take(), AfterTake::Swap);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn with_nothing_written_preview_holds_the_scene_put_back_on_air() {
        let s = server();
        restore(&s, None, &OnAir::Scene("Scene 2".into()));
        assert_eq!(armed_name(&s).as_deref(), Some("Scene 2"));
    }

    #[test]
    fn a_scene_deleted_since_gives_way_to_the_one_on_air() {
        let s = server();
        restore(&s, Some(Armed { scene: Some("gone".into()), after_take: AfterTake::Same }), &OnAir::Scene("Scene 1".into()));
        assert_eq!(armed_name(&s).as_deref(), Some("Scene 1"));
    }

    #[test]
    fn nothing_armed_before_stays_unarmed() {
        let s = server();
        restore(&s, Some(Armed { scene: None, after_take: AfterTake::Same }), &OnAir::Scene("Scene 1".into()));
        assert_eq!(s.armed(), None);
    }
}
