//! Several clients on one document: per client undo, conflicts, transactions
//! that hold nobody else up, and drafts that refuse to overwrite.

use super::*;
use crate::scene::document::{Content, Frame, Item, Scene, Vec2};

pub(super) const PHONE: Option<&str> = Some("default.phone");
pub(super) const LAPTOP: Option<&str> = Some("default.laptop");

pub(super) fn server() -> Arc<SceneServer> {
    SceneServer::in_memory(CanvasCaps::new(&crate::config::Canvas {
        width: 1920,
        height: 1080,
        fps: 30,
        sample_rate: 48000,
        channels: 2,
    }))
}

/// A scene called "two" with boxes "left" and "right", added by nobody.
pub(super) fn two_box(s: &SceneServer) {
    s.edit(None, |doc| {
        let mut scene = Scene::new("two");
        for name in ["left", "right"] {
            let mut item = Item::new(Content::Source { source: format!("cam-{name}") });
            item.name = Some(name.into());
            item.transform.frame = Some(Frame::new(960.0, 540.0));
            scene.items.push(item);
        }
        doc.scenes.push(scene);
        Ok(())
    })
    .expect("adding a scene");
}

pub(super) fn move_to(s: &SceneServer, client: Option<&str>, item: &str, x: f64) {
    s.edit_scene(client, "two", |doc, i| {
        let id = find::item_id_in(&doc.scenes[i], item)?;
        ops::item_mut(&mut doc.scenes[i].items, id).unwrap().transform.position = Vec2::new(x, 0.0);
        Ok(())
    })
    .expect("a move");
}

pub(super) fn x_of(s: &SceneServer, item: &str) -> f64 {
    let view = s.scene("two").unwrap();
    let id = view.records.iter().find(|r| match &r.props {
        crate::scene::flat::Props::Item(p) => p.name.as_deref() == Some(item),
        _ => false,
    });
    let id = id.expect("the item").id;
    view.geometry.iter().find(|g| g.item == id).expect("its box").x
}

#[test]
fn undo_takes_back_only_the_asking_clients_own_change() {
    let s = server();
    two_box(&s);
    move_to(&s, PHONE, "left", 100.0);
    move_to(&s, LAPTOP, "right", 1200.0);

    s.undo(PHONE, false).expect("the phone undoes its own move");
    assert_eq!(x_of(&s, "left"), 0.0, "the phone's move was not taken back");
    assert_eq!(x_of(&s, "right"), 1200.0, "the phone's undo took back the laptop's move");
    assert_eq!(s.history(PHONE), (0, 1));
    assert_eq!(s.history(LAPTOP), (1, 0), "the laptop's stack was touched");

    let err = s.undo(PHONE, false).expect_err("the phone has nothing left");
    assert!(format!("{err}").contains("nothing to undo"), "{err}");
}

#[test]
fn an_undo_over_somebody_elses_later_change_is_refused_and_names_them() {
    let s = server();
    two_box(&s);
    move_to(&s, PHONE, "left", 100.0);
    move_to(&s, LAPTOP, "left", 300.0);

    let err = s.undo(PHONE, false).expect_err("the laptop moved it since");
    let refused = err.downcast_ref::<Refused>().expect("a typed refusal");
    assert_eq!(refused.conflicts.len(), 1);
    assert_eq!(refused.conflicts[0].name.as_deref(), Some("left"));
    assert_eq!(refused.conflicts[0].changed_by.as_deref(), LAPTOP);
    let text = err.to_string();
    assert!(text.contains("default.laptop") && text.contains("force: true"), "{text}");
    assert_eq!(x_of(&s, "left"), 300.0, "a refused undo changed the document");
    assert_eq!(s.history(PHONE).0, 1, "the refused step has to stay on the stack");

    s.undo(PHONE, true).expect("forced, it goes through");
    assert_eq!(x_of(&s, "left"), 0.0, "force puts the phone's version back");
}

#[test]
fn an_undo_whose_scene_was_removed_is_refused_as_gone() {
    let s = server();
    two_box(&s);
    move_to(&s, PHONE, "left", 100.0);
    s.edit(LAPTOP, |doc| {
        doc.scenes.clear();
        Ok(())
    })
    .unwrap();
    let err = s.undo(PHONE, false).expect_err("the item is gone");
    let refused = err.downcast_ref::<Refused>().expect("a typed refusal");
    assert!(refused.conflicts[0].gone, "{:?}", refused.conflicts);
    // Forced, there is nowhere to put it back, so nothing comes back and
    // nothing fails.
    s.undo(PHONE, true).expect("a forced undo of something gone is not an error");
    assert!(s.document().scenes.is_empty());
}

#[test]
fn an_open_transaction_holds_nobody_else_up() {
    let s = server();
    two_box(&s);
    let mut rx = s.subscribe();
    s.begin(PHONE).expect("the phone opens one");
    s.begin(LAPTOP).expect("a second client's transaction is not refused");
    s.abort(LAPTOP).unwrap();

    move_to(&s, PHONE, "left", 100.0);
    assert!(rx.try_recv().is_err(), "half the phone's batch was published");
    move_to(&s, LAPTOP, "right", 1200.0);
    let theirs = rx.try_recv().expect("the laptop's edit is published at once");
    assert_eq!(theirs.source_client.as_deref(), LAPTOP);

    let batch = s.commit(PHONE).expect("committing");
    assert_eq!(batch.seq, theirs.seq + 1, "the batch is the next number, with no gap");
    s.undo(LAPTOP, false).expect("the laptop's undo is its own move");
    assert_eq!((x_of(&s, "left"), x_of(&s, "right")), (100.0, 0.0));
}

#[test]
fn an_abort_leaves_alone_what_somebody_else_changed_since() {
    let s = server();
    two_box(&s);
    s.begin(PHONE).unwrap();
    move_to(&s, PHONE, "left", 100.0);
    move_to(&s, PHONE, "right", 50.0);
    move_to(&s, LAPTOP, "right", 1200.0);
    s.abort(PHONE).expect("throwing the phone's batch away");
    assert_eq!(x_of(&s, "left"), 0.0, "the phone's own move stayed");
    assert_eq!(x_of(&s, "right"), 1200.0, "the abort undid the laptop's move");
}
