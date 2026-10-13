//! The Default scene a new collection starts with.
//!
//! Made once, when the collection has no scenes at all. A scene somebody has
//! emptied is still a scene, so it is never filled again: the sources taken
//! off it stay off across a reload and across a restart of the core.

use anyhow::Result;

use super::{ops, SceneServer};
use crate::scene::document::Scene;

/// The name the first scene goes by.
const DEFAULT_SCENE: &str = "Default scene";

impl SceneServer {
    /// Add the Default scene, drawing `sources`, when there are no scenes.
    /// Returns whether it made one.
    pub fn seed_default(&self, sources: &[String]) -> Result<bool> {
        if !self.list().is_empty() {
            return Ok(false);
        }
        self.edit(None, |doc| {
            let scene = if sources.is_empty() {
                Scene::new(DEFAULT_SCENE)
            } else {
                ops::create_from(doc, sources, None, Some(DEFAULT_SCENE))?
            };
            doc.scenes.push(scene);
            Ok(())
        })?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::CanvasCaps;
    use crate::scene::server::find;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn caps() -> CanvasCaps {
        CanvasCaps::new(&crate::config::Canvas { width: 1280, height: 720, fps: 30, sample_rate: 48000, channels: 2 })
    }

    fn sources() -> Vec<String> {
        vec!["clip".into(), "news".into()]
    }

    /// A core's scene server on a runtime store in its own folder.
    fn open(dir: &std::path::Path) -> Arc<SceneServer> {
        let server = SceneServer::open(Some(dir.join("godwinmix.runtime.toml")), caps()).expect("opening");
        server.seed_default(&sources()).expect("seeding");
        server
    }

    /// Every item off the scene, one `scene.item.remove` at a time.
    fn remove_every_item(server: &SceneServer) {
        let ids: Vec<String> = server.document().scenes[0].walk().iter().map(|i| i.id.to_string()).collect();
        assert_eq!(ids.len(), 2, "the Default scene starts with both sources");
        for item in ids {
            server
                .edit_scene(Some("ui"), DEFAULT_SCENE, |doc, i| {
                    let id = find::item_id_in(&doc.scenes[i], &item)?;
                    ops::take_item(&mut doc.scenes[i].items, id);
                    Ok(())
                })
                .expect("removing an item");
        }
    }

    fn assert_empty(server: &SceneServer, when: &str) {
        let list = server.list();
        assert_eq!(list.len(), 1, "{when}: still one scene");
        assert_eq!(list[0].name, DEFAULT_SCENE);
        assert_eq!(list[0].items, 0, "{when}: no items");
        assert!(list[0].sources.is_empty(), "{when}: draws no sources, got {:?}", list[0].sources);
        assert_eq!(server.scene(DEFAULT_SCENE).unwrap().records.len(), 1, "{when}: only the scene record");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gmx-seed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_scene_with_every_source_removed_reads_back_empty() {
        let server = SceneServer::in_memory(caps());
        assert!(server.seed_default(&sources()).unwrap(), "a new collection gets its Default scene");
        remove_every_item(&server);
        assert_empty(&server, "read straight back");
        assert!(!server.seed_default(&sources()).unwrap(), "an emptied scene is not filled again");
        assert_empty(&server, "after seeding again");
    }

    #[test]
    fn an_emptied_scene_stays_empty_across_a_restart() {
        let dir = scratch("restart");
        let first = open(&dir);
        remove_every_item(&first);
        assert_empty(&first, "before the restart");
        drop(first);

        let second = open(&dir);
        assert_empty(&second, "after the restart");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
