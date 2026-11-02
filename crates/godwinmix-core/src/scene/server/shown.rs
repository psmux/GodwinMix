//! A draft drawn in Preview in place of the armed scene, and who asked.
//!
//! A designer moves boxes on a draft, and a draft is in no picture the core
//! makes: the armed scene's preview shows the scene as saved and the
//! programme shows what is on air. So the composer points the one preview
//! compositor at its draft, through the same placements the programme would
//! get, and what is seen is what Apply gives.
//!
//! That hold on Preview is everybody's Preview, so it has to end when the
//! designer has gone as well as when they press Discard. It ends when the
//! draft is applied or discarded (the layout then finds no draft), when
//! the designer arms a scene, or anybody arms one while nobody known is
//! showing it (`SceneServer::arm_for`), when anybody asks
//! `scene.preview.set {"draft": ""}`, and when the client that asked leaves
//! with no other connection left open (`client_left`). The draft itself is
//! kept: it is a working copy, and the designer can open it again.

use super::{drafts, SceneServer};
use anyhow::Result;

impl SceneServer {
    /// Have the preview draw a draft in place of the armed scene, or stop.
    ///
    /// What is armed is left alone, because `program.take` with no argument
    /// takes it and a designer opening must not change what a take does.
    pub fn show_draft(&self, draft: Option<&str>) -> Result<()> {
        self.show_draft_for(draft, None)
    }

    /// The same, remembering which client asked, so the hold can end when
    /// that client goes.
    pub fn show_draft_for(&self, draft: Option<&str>, client: Option<&str>) -> Result<()> {
        let mut inner = self.inner.lock();
        inner.preview_draft = match draft {
            Some(id) => Some(drafts::find_draft(&inner.drafts, id)?.id),
            None => None,
        };
        inner.preview_draft_by = inner.preview_draft.and(client.map(str::to_string));
        Ok(())
    }

    /// The client that asked for the draft Preview is drawing, if one is.
    pub fn draft_shown_by(&self) -> Option<String> {
        let inner = self.inner.lock();
        inner.preview_draft.and(inner.preview_draft_by.clone())
    }

    /// A client has gone and left no connection open. If it was holding
    /// Preview on its draft, Preview goes back to the armed scene; true when
    /// it did, so the caller can redraw it.
    pub fn client_left(&self, client: &str) -> bool {
        let mut inner = self.inner.lock();
        if inner.preview_draft.is_none() || inner.preview_draft_by.as_deref() != Some(client) {
            return false;
        }
        inner.preview_draft = None;
        inner.preview_draft_by = None;
        tracing::info!(client, "the client showing a draft in Preview left; Preview shows the armed scene again");
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::caps::CanvasCaps;
    use crate::scene::document::{Content, Item, Scene};
    use crate::scene::server::SceneServer;

    fn server_with_scene() -> std::sync::Arc<SceneServer> {
        let canvas = CanvasCaps::new(&crate::config::Canvas {
            width: 1920,
            height: 1080,
            fps: crate::config::FrameRate::whole(30),
            sample_rate: 48000,
            channels: 2,
        });
        let s = SceneServer::in_memory(canvas);
        s.edit(None, |doc| {
            let mut scene = Scene::new("one");
            scene.items.push(Item::new(Content::Source { source: "cam1".into() }));
            doc.scenes.push(scene);
            Ok(())
        })
        .unwrap();
        s
    }

    /// The rig's report: a composer left open by a browser that closed kept
    /// Preview on its draft for everyone until somebody pressed Discard.
    #[test]
    fn a_draft_shown_by_a_client_that_left_lets_go_of_preview() {
        let s = server_with_scene();
        let draft = s.edit_begin(Some("laptop"), "one", false).unwrap().id.to_string();
        s.show_draft_for(Some(&draft), Some("laptop")).unwrap();
        assert_eq!(s.draft_shown_by().as_deref(), Some("laptop"));
        assert!(s.preview_layout(320, 180).is_some(), "the draft is drawn");
        assert!(!s.client_left("phone"), "somebody else leaving changes nothing");
        assert!(s.preview_layout(320, 180).is_some());
        assert!(s.client_left("laptop"));
        assert!(s.preview_layout(320, 180).is_none(), "nothing is armed, so nothing is drawn");
        assert!(s.draft(&draft).is_ok(), "the draft itself is kept to be opened again");
    }

    #[test]
    fn arming_a_scene_takes_preview_back_from_a_shown_draft() {
        let s = server_with_scene();
        let draft = s.edit_begin(None, "one", false).unwrap().id.to_string();
        s.show_draft(Some(&draft)).unwrap();
        s.edit_draft(&draft, |doc, i| {
            doc.scenes[i].items.clear();
            Ok(())
        })
        .unwrap();
        assert_eq!(s.preview_layout(320, 180).unwrap().cells.len(), 0, "the empty draft is drawn");
        s.arm(Some("one")).unwrap();
        assert_eq!(s.preview_layout(320, 180).unwrap().cells.len(), 1, "the armed scene is drawn, not the draft");
        assert!(s.draft_shown_by().is_none());
    }

    /// Another operator arming the next shot does not take Preview off a
    /// draft a designer is laying out; the designer arming one does.
    #[test]
    fn another_operator_arming_leaves_a_designers_draft_shown() {
        let s = server_with_scene();
        let draft = s.edit_begin(None, "one", false).unwrap().id.to_string();
        s.show_draft_for(Some(&draft), Some("laptop")).unwrap();
        s.arm_for(Some("one"), Some("phone")).unwrap();
        assert_eq!(s.draft_shown_by().as_deref(), Some("laptop"), "the phone's arm took the designer's draft away");
        s.arm_for(Some("one"), Some("laptop")).unwrap();
        assert!(s.draft_shown_by().is_none(), "the designer's own arm leaves the draft shown");
    }

    #[test]
    fn an_empty_draft_name_stops_showing_one() {
        let s = server_with_scene();
        let draft = s.edit_begin(None, "one", false).unwrap().id.to_string();
        s.show_draft(Some(&draft)).unwrap();
        s.show_draft(None).unwrap();
        assert!(s.preview_layout(320, 180).is_none());
    }
}
