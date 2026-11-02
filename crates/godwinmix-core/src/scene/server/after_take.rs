//! What stays armed after a scene is taken by name.
//!
//! A take of the armed scene leaves it armed, so Preview keeps the shot that
//! just went to air, as Wirecast does. A take by name (a Stream Deck,
//! Companion, an agent, a tap on a scene in the Scenes panel) used to leave
//! the old scene armed, so Preview went on showing the shot that had just
//! left air. Every open page then had to arm the new one itself, and with no
//! page open nobody did.
//!
//! So the core moves the armed scene along with the take, but only when
//! Preview was following the programme: nothing was armed, or what was armed
//! is what was on air before. A scene another operator has armed to go next
//! is theirs and stays. A draft shown in Preview stays too, because a
//! designer is looking at it.

use super::{find, Id, SceneServer};

impl SceneServer {
    /// Arm `taken` after it went to air, if Preview was following the
    /// programme: nothing armed, or `was_on_air` armed. Returns the newly
    /// armed scene's name when this changed what is armed, for the caller to
    /// announce.
    pub fn follow_take(&self, taken: &str, was_on_air: Option<Id>) -> Option<String> {
        let mut inner = self.inner.lock();
        let scene = find::scene(&inner.doc, taken).ok()?;
        let (id, name) = (scene.id, scene.name.clone());
        let following = match inner.preview {
            None => true,
            Some(armed) => armed != id && Some(armed) == was_on_air,
        };
        if !following {
            return None;
        }
        inner.preview = Some(id);
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use crate::caps::CanvasCaps;
    use crate::scene::document::{Content, Item, Scene};
    use crate::scene::server::SceneServer;

    fn server() -> std::sync::Arc<SceneServer> {
        let s = SceneServer::in_memory(CanvasCaps::new(&crate::config::Canvas {
            width: 1920,
            height: 1080,
            fps: crate::config::FrameRate::whole(30),
            sample_rate: 48000,
            channels: 2,
        }));
        s.edit(None, |doc| {
            for name in ["wide", "two", "close"] {
                let mut scene = Scene::new(name);
                scene.items.push(Item::new(Content::Source { source: "cam1".into() }));
                doc.scenes.push(scene);
            }
            Ok(())
        })
        .unwrap();
        s
    }

    fn id(s: &SceneServer, name: &str) -> crate::scene::id::Id {
        s.scene(name).unwrap().id
    }

    #[test]
    fn a_take_by_name_with_nothing_armed_arms_the_scene_taken() {
        let s = server();
        assert_eq!(s.follow_take("two", None).as_deref(), Some("two"));
        assert_eq!(s.armed(), Some(id(&s, "two")));
    }

    /// Preview held what was on air; a Stream Deck took another scene.
    #[test]
    fn a_take_by_name_moves_the_armed_scene_when_it_was_the_one_on_air() {
        let s = server();
        s.arm(Some("wide")).unwrap();
        let wide = id(&s, "wide");
        assert_eq!(s.follow_take("two", Some(wide)).as_deref(), Some("two"));
        assert_eq!(s.armed(), Some(id(&s, "two")));
    }

    /// Another operator armed Close to go next; a take of Two by name leaves
    /// their choice alone.
    #[test]
    fn a_scene_armed_to_go_next_is_left_armed() {
        let s = server();
        s.arm(Some("close")).unwrap();
        assert!(s.follow_take("two", Some(id(&s, "wide"))).is_none());
        assert_eq!(s.armed(), Some(id(&s, "close")));
    }

    #[test]
    fn the_armed_scene_taken_stays_armed_and_nothing_is_announced() {
        let s = server();
        s.arm(Some("two")).unwrap();
        assert!(s.follow_take("two", Some(id(&s, "wide"))).is_none());
        assert_eq!(s.armed(), Some(id(&s, "two")));
    }

    #[test]
    fn a_draft_shown_in_preview_stays_shown() {
        let s = server();
        let draft = s.edit_begin(None, "close", false).unwrap().id.to_string();
        s.show_draft(Some(&draft)).unwrap();
        s.follow_take("two", None);
        assert_eq!(s.draft_shown_by(), None);
        assert_eq!(s.preview_layout(320, 180).unwrap().name, "close", "the designer's draft is still what Preview draws");
    }
}
