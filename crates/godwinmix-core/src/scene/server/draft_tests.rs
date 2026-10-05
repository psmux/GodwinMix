//! Drafts with several people: an apply over a scene somebody changed since
//! is refused, one over an untouched scene is not, and a take leaves a stale
//! draft open.

use super::collab_tests::{move_to, server, two_box, x_of, LAPTOP, PHONE};
use super::*;
use crate::scene::document::Scene;

#[test]
fn a_draft_over_a_scene_that_changed_since_is_refused_with_what_changed() {
    let s = server();
    two_box(&s);
    let draft = s.edit_begin(PHONE, "two", false).unwrap();
    assert_eq!(draft.owner.as_deref(), PHONE);
    s.edit_draft(&draft.id.to_string(), |doc, i| {
        doc.scenes[i].items.truncate(1);
        Ok(())
    })
    .unwrap();
    move_to(&s, LAPTOP, "right", 1200.0);

    let err = s.edit_apply(PHONE, &draft.id.to_string(), false).expect_err("stale");
    let stale = err.downcast_ref::<Stale>().expect("a typed refusal");
    assert_eq!(stale.base_seq, draft.base_seq);
    assert!(stale.seq > stale.base_seq);
    assert_eq!(stale.changes.len(), 1, "{:?}", stale.changes);
    assert_eq!(stale.changes[0].changed_by.as_deref(), LAPTOP);
    assert!(err.to_string().contains("scene.edit.discard"), "{err}");
    assert_eq!(s.scene("two").unwrap().geometry.len(), 2, "a refused apply changed the scene");
    assert_eq!(s.drafts().len(), 1, "a refused draft has to stay open");

    s.edit_apply(PHONE, &draft.id.to_string(), true).expect("forced");
    assert_eq!(s.scene("two").unwrap().geometry.len(), 1, "force replaces the scene");
}

#[test]
fn a_draft_over_an_untouched_scene_applies_after_unrelated_edits() {
    let s = server();
    two_box(&s);
    s.edit(None, |doc| {
        doc.scenes.push(Scene::new("other"));
        Ok(())
    })
    .unwrap();
    let draft = s.edit_begin(PHONE, "two", false).unwrap();
    s.edit(LAPTOP, |doc| {
        doc.scenes.push(Scene::new("third"));
        Ok(())
    })
    .unwrap();
    s.edit_apply(PHONE, &draft.id.to_string(), false).expect("nothing in this scene changed");
}

#[test]
fn a_take_leaves_a_stale_draft_open_rather_than_overwriting() {
    let s = server();
    two_box(&s);
    let scene = s.scene("two").unwrap().id;
    let draft = s.edit_begin(PHONE, "two", false).unwrap();
    move_to(&s, LAPTOP, "left", 400.0);
    assert!(s.apply_drafts_of(LAPTOP, scene).is_empty(), "a stale draft was applied by the take");
    assert_eq!(x_of(&s, "left"), 400.0);
    assert!(s.draft(&draft.id.to_string()).is_ok(), "the draft should still be open");
}
