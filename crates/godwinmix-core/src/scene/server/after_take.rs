//! What stays armed after a scene is taken.
//!
//! The core holds one setting for it, `AfterTake`, which a page's "After
//! Take, Preview shows" choice sets (`scene.preview.set` with `after_take`).
//! It is the core's and not each page's because `program.take {}`, the
//! Take on a Stream Deck through Companion, takes whatever is armed here,
//! and it has to take the same thing whichever page last drew Preview.
//!
//! `Same`, the default: Preview keeps the shot that just went to air, as
//! Wirecast does. A take of the armed scene leaves it armed. A take by name
//! (a Stream Deck, Companion, an agent, a tap on a scene in the Scenes panel)
//! arms the scene taken when Preview was following the programme: nothing
//! was armed, or what was armed is what was on air before. So
//! `program.take {}` after a Stream Deck take of Two takes Two again, which
//! is a cut to itself and changes nothing.
//!
//! `Swap`, a vision mixer's buses: a take of the armed scene arms the scene
//! that was on air before it, so Take and Take again go back and forth
//! between two shots, and `program.take {}` takes the shot that just left
//! air. A take by name of any other scene leaves what is armed alone, as a
//! programme bus button leaves the preview bus alone.
//!
//! In both, a scene another operator has armed to go next is theirs and
//! stays, and a draft shown in Preview stays, because a designer is looking
//! at it.
//!
//! The comparison with what was armed and the change are one step under the
//! server's lock, so an operator arming a scene between the take and this
//! wins. Two takes by name at once are put in order by the caller
//! (`program::take_scene` holds its take lock across the mixer request and
//! this call), so each reads what the one before it put on air.

use super::{find, Id, SceneServer};
use serde::{Deserialize, Serialize};

/// What Preview holds after a Take. See the module.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AfterTake {
    #[default]
    Same,
    Swap,
}

impl SceneServer {
    pub fn after_take(&self) -> AfterTake {
        self.inner.lock().after_take
    }

    pub fn set_after_take(&self, mode: AfterTake) {
        self.inner.lock().after_take = mode;
    }

    /// What is armed after `taken` went to air in place of `was_on_air`, by
    /// the core's `AfterTake`. Returns the newly armed scene's name when this
    /// changed what is armed, for the caller to announce.
    pub fn follow_take(&self, taken: &str, was_on_air: Option<Id>) -> Option<String> {
        let mut inner = self.inner.lock();
        let id = find::scene(&inner.doc, taken).ok()?.id;
        let armed = inner.preview;
        let next = match inner.after_take {
            AfterTake::Same => {
                let following = armed.is_none() || (armed != Some(id) && armed == was_on_air);
                following.then_some(id)
            }
            AfterTake::Swap => was_on_air.filter(|was| armed == Some(id) && *was != id),
        }?;
        let name = inner.doc.scene(&next)?.name.clone();
        inner.preview = Some(next);
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

    /// Swap: Take of the armed scene arms the one that left air, so Take
    /// again goes back. A take by name of another scene leaves the armed one.
    #[test]
    fn swap_arms_the_scene_that_left_air_and_a_take_by_name_leaves_preview_alone() {
        let s = server();
        s.set_after_take(super::AfterTake::Swap);
        s.arm(Some("two")).unwrap();
        let wide = id(&s, "wide");
        assert_eq!(s.follow_take("two", Some(wide)).as_deref(), Some("wide"));
        assert_eq!(s.armed(), Some(wide));
        assert!(s.follow_take("close", Some(id(&s, "two"))).is_none());
        assert_eq!(s.armed(), Some(wide), "a programme bus button leaves the preview bus alone");
        // Take again: Wide goes up and Close, which it replaced, is armed.
        assert_eq!(s.follow_take("wide", Some(id(&s, "close"))).as_deref(), Some("close"));
    }

    /// An operator armed Close after the take was asked and before it was
    /// followed: what they armed stays.
    #[test]
    fn an_arm_between_the_take_and_the_follow_wins() {
        let s = server();
        s.arm(Some("wide")).unwrap();
        let wide = id(&s, "wide");
        s.arm(Some("close")).unwrap();
        assert!(s.follow_take("two", Some(wide)).is_none());
        assert_eq!(s.armed(), Some(id(&s, "close")));
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
