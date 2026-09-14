//! The scene server, tested without a pipeline.
//!
//! Everything here works on the document, which is what lets these run in
//! milliseconds and on a machine with no GStreamer plugins worth the name. The
//! pipeline half is tested in `mixer.rs` against real sources.

use super::*;
use crate::scene::document::{Content, Fit, Frame, Item, Vec2};

fn caps() -> CanvasCaps {
    CanvasCaps::new(&crate::config::Canvas {
        width: 1920,
        height: 1080,
        fps: 30,
        sample_rate: 48000,
        channels: 2,
    })
}

fn server() -> Arc<SceneServer> {
    SceneServer::in_memory(caps())
}

/// A scene of two named boxes, which is what most of these want.
fn two_box(s: &SceneServer) -> SceneView {
    s.edit(None, |doc| {
        let mut scene = Scene::new("two");
        for (name, source, x) in [("left", "cam1", 0.0), ("right", "cam2", 960.0)] {
            let mut item = Item::new(Content::Source { source: source.into() });
            item.name = Some(name.into());
            item.transform.position = Vec2::new(x, 0.0);
            item.transform.frame = Some(Frame::new(960.0, 540.0));
            scene.items.push(item);
        }
        doc.scenes.push(scene);
        Ok(())
    })
    .expect("adding a scene");
    s.scene("two").expect("it is there")
}

#[test]
fn a_change_produces_one_patch_and_the_records_it_touched() {
    let s = server();
    let view = two_box(&s);
    assert_eq!(view.records.len(), 3, "the scene record and its two items");
    assert_eq!(view.geometry.len(), 2);
    assert_eq!(view.geometry[0].width, 960.0);

    let mut rx = s.subscribe();
    let out = s
        .edit_scene(Some("client-a"), "two", |doc, i| {
            let id = find::item_id_in(&doc.scenes[i], "left")?;
            ops::item_mut(&mut doc.scenes[i].items, id).unwrap().transform.position.x = 40.0;
            Ok(())
        })
        .expect("moving an item");
    assert_eq!(out.patch.updated.len(), 1, "moving one item writes one record");
    assert_eq!(out.patch.source_client.as_deref(), Some("client-a"));
    let published = rx.try_recv().expect("the patch reached a subscriber");
    assert_eq!(published.seq, out.patch.seq);
    assert_eq!(out.scene.unwrap().geometry[0].x, 40.0, "the answer carries the new geometry");
}

#[test]
fn a_command_that_changes_nothing_produces_no_patch() {
    let s = server();
    two_box(&s);
    let mut rx = s.subscribe();
    let out = s.edit_scene(None, "two", |_doc, _i| Ok(())).expect("a command that does nothing");
    assert!(out.patch.is_empty());
    assert!(rx.try_recv().is_err(), "an empty change was published");
}

/// The acceptance line: undo after a drag restores the exact transform.
#[test]
fn undo_after_a_drag_restores_the_exact_transform() {
    let s = server();
    two_box(&s);
    let before = s.scene("two").unwrap().geometry[0].clone();

    // A drag: one mark, then forty moves at input rate.
    s.mark(Some("drag left".into()));
    for step in 1..=40 {
        s.edit_scene(Some("designer"), "two", |doc, i| {
            let id = find::item_id_in(&doc.scenes[i], "left")?;
            let item = ops::item_mut(&mut doc.scenes[i].items, id).unwrap();
            item.transform.position = Vec2::new(step as f64 * 3.0, step as f64);
            Ok(())
        })
        .expect("a move");
    }
    s.mark(None);
    let dragged = s.scene("two").unwrap().geometry[0].clone();
    assert_eq!((dragged.x, dragged.y), (120.0, 40.0));
    assert_eq!(s.history().0, 2, "the drag is one step, on top of the scene being added");

    s.undo(None).expect("one Ctrl+Z");
    let after = s.scene("two").unwrap().geometry[0].clone();
    assert_eq!(
        (after.x, after.y),
        (before.x, before.y),
        "undo after a drag has to restore the exact transform, not an approximation"
    );

    s.redo(None).expect("and forward again");
    let again = s.scene("two").unwrap().geometry[0].clone();
    assert_eq!((again.x, again.y), (dragged.x, dragged.y));
}

#[test]
fn undo_with_nothing_on_the_stack_says_so() {
    let s = server();
    let err = s.undo(None).expect_err("nothing has been done");
    assert!(format!("{err}").contains("nothing to undo"), "{err}");
}

#[test]
fn a_transaction_applies_on_one_frame_or_not_at_all() {
    let s = server();
    two_box(&s);
    let mut rx = s.subscribe();

    s.begin().expect("opening one");
    assert!(s.in_transaction());
    for name in ["left", "right"] {
        s.edit_scene(None, "two", |doc, i| {
            let id = find::item_id_in(&doc.scenes[i], name)?;
            ops::item_mut(&mut doc.scenes[i].items, id).unwrap().opacity = 0.5;
            Ok(())
        })
        .expect("a change inside the transaction");
    }
    assert!(rx.try_recv().is_err(), "half a batch reached a client");

    let patch = s.commit(None).expect("committing");
    assert_eq!(patch.updated.len(), 2, "the whole batch is one patch");
    assert_eq!(rx.try_recv().expect("the batch was published").seq, patch.seq);
    assert!(rx.try_recv().is_err(), "the batch was published more than once");

    // And it is one step on the undo stack.
    s.undo(None).expect("one Ctrl+Z for the batch");
    let view = s.scene("two").unwrap();
    for g in &view.geometry {
        assert_eq!(g.opacity, 1.0, "undoing the batch left half of it behind");
    }
}

#[test]
fn an_aborted_transaction_puts_everything_back() {
    let s = server();
    two_box(&s);
    s.begin().unwrap();
    s.edit_scene(None, "two", |doc, i| {
        doc.scenes[i].items.push(Item::new(Content::Source { source: "cam3".into() }));
        Ok(())
    })
    .unwrap();
    assert_eq!(s.scene("two").unwrap().geometry.len(), 3);
    s.abort(None).expect("throwing it away");
    assert_eq!(s.scene("two").unwrap().geometry.len(), 2, "the abort left something behind");
    assert!(!s.in_transaction());
}

#[test]
fn two_transactions_at_once_are_refused_with_what_to_do() {
    let s = server();
    s.begin().unwrap();
    let err = s.begin().expect_err("only one at a time");
    assert!(format!("{err}").contains("scene.transaction.commit"), "{err}");
    s.abort(None).unwrap();
}

#[test]
fn a_draft_is_edited_off_air_and_applied_when_it_is_asked_for() {
    let s = server();
    two_box(&s);
    let mut rx = s.subscribe();
    let draft = s.edit_begin("two", false).expect("taking a working copy");

    let view = s
        .edit_draft(&draft.id.to_string(), |doc, i| {
            let id = find::item_id_in(&doc.scenes[i], "left")?;
            ops::item_mut(&mut doc.scenes[i].items, id).unwrap().transform.position.x = 500.0;
            Ok(())
        })
        .expect("editing the draft");
    assert_eq!(view.geometry[0].x, 500.0, "the draft carries the change");
    assert_eq!(s.scene("two").unwrap().geometry[0].x, 0.0, "the live scene moved");
    assert!(rx.try_recv().is_err(), "a draft edit was published");

    s.edit_apply(None, &draft.id.to_string()).expect("applying it");
    assert_eq!(s.scene("two").unwrap().geometry[0].x, 500.0);
    assert!(rx.try_recv().is_ok(), "applying a draft has to publish");
    assert!(s.drafts().is_empty(), "the draft outlived its apply");
}

#[test]
fn a_discarded_draft_changes_nothing() {
    let s = server();
    two_box(&s);
    let draft = s.edit_begin("two", false).unwrap();
    s.edit_draft(&draft.id.to_string(), |doc, i| {
        doc.scenes[i].items.clear();
        Ok(())
    })
    .unwrap();
    s.edit_discard(&draft.id.to_string()).expect("throwing it away");
    assert_eq!(s.scene("two").unwrap().geometry.len(), 2);
    let err = s.draft(&draft.id.to_string()).expect_err("it is gone");
    assert!(format!("{err}").contains("scene.edit.begin"), "{err}");
}

#[test]
fn a_draft_of_the_scene_going_to_air_is_applied_by_the_take() {
    let s = server();
    two_box(&s);
    let scene_id = s.scene("two").unwrap().id;
    let draft = s.edit_begin("two", false).unwrap();
    s.edit_draft(&draft.id.to_string(), |doc, i| {
        let id = find::item_id_in(&doc.scenes[i], "right")?;
        ops::item_mut(&mut doc.scenes[i].items, id).unwrap().opacity = 0.25;
        Ok(())
    })
    .unwrap();
    let applied = s.apply_drafts_of(None, scene_id);
    assert_eq!(applied.len(), 1, "the waiting draft was not applied by the take");
    assert_eq!(s.scene("two").unwrap().geometry[1].opacity, 0.25);
}

#[test]
fn arming_a_scene_makes_it_the_preview_and_gives_a_layout_for_it() {
    let s = server();
    two_box(&s);
    assert!(s.armed().is_none());
    assert!(s.preview_layout(320, 180).is_none(), "nothing is composited while nothing is armed");

    let summary = s.arm(Some("two")).expect("arming").expect("a scene came back");
    assert!(summary.armed);
    assert_eq!(s.armed(), Some(summary.id));

    let layout = s.preview_layout(320, 180).expect("an armed scene has a layout");
    assert_eq!(layout.cells.len(), 2);
    assert_eq!(layout.cells[0].width, 160, "the layout is scaled to the size asked for");
    assert_eq!(layout.cells[1].x, 160);

    s.arm(None).expect("disarming");
    assert!(s.preview_layout(320, 180).is_none());
}

#[test]
fn a_scene_named_by_name_or_by_id_is_the_same_scene() {
    let s = server();
    let view = two_box(&s);
    assert_eq!(s.scene(&view.id.to_string()).unwrap().id, view.id);
    let err = s.scene("nope").expect_err("no such scene");
    assert!(format!("{err}").contains("two"), "the error has to list what exists: {err}");
}

#[test]
fn the_placements_a_scene_becomes_are_the_boxes_it_describes() {
    let s = server();
    two_box(&s);
    let (name, placements) = s.placements("two").expect("flattening");
    assert_eq!(name, "two");
    assert_eq!(placements.len(), 2);
    assert_eq!((placements[0].xpos, placements[0].width), (0, 960));
    assert_eq!((placements[1].xpos, placements[1].width), (960, 960));
}

// -- the semantic operations ------------------------------------------

#[test]
fn align_puts_every_named_item_on_one_edge() {
    let s = server();
    two_box(&s);
    s.edit_scene(None, "two", |doc, i| {
        let ids = ops::ids(&doc.scenes[i], &["left".into(), "right".into()])?;
        ops::align(doc, i, &ids, ops::Edge::Left)
    })
    .expect("aligning left");
    let view = s.scene("two").unwrap();
    assert_eq!(view.geometry[0].x, view.geometry[1].x, "the two are not on one edge");
    assert_eq!(view.geometry[0].x, 0.0, "align left goes to the leftmost, not to zero by luck");
}

#[test]
fn aligning_one_item_says_what_it_needs() {
    let s = server();
    two_box(&s);
    let err = s
        .edit_scene(None, "two", |doc, i| {
            let ids = ops::ids(&doc.scenes[i], &["left".into()])?;
            ops::align(doc, i, &ids, ops::Edge::Left)
        })
        .expect_err("one item cannot be aligned with itself");
    assert!(format!("{err}").contains("at least two"), "{err}");
}

#[test]
fn distribute_leaves_even_gaps_between_items_of_different_sizes() {
    let s = server();
    s.edit(None, |doc| {
        let mut scene = Scene::new("row");
        for (name, x, w) in [("a", 0.0, 100.0), ("b", 300.0, 400.0), ("c", 1000.0, 100.0)] {
            let mut item = Item::new(Content::Source { source: "cam1".into() });
            item.name = Some(name.into());
            item.transform.position = Vec2::new(x, 0.0);
            item.transform.frame = Some(Frame::new(w, 100.0));
            scene.items.push(item);
        }
        doc.scenes.push(scene);
        Ok(())
    })
    .unwrap();
    s.edit_scene(None, "row", |doc, i| {
        let ids = ops::ids(&doc.scenes[i], &["a".into(), "b".into(), "c".into()])?;
        ops::distribute(doc, i, &ids, ops::Axis::Horizontal)
    })
    .expect("distributing");
    let g = s.scene("row").unwrap().geometry;
    let gap1 = g[1].x - (g[0].x + g[0].width);
    let gap2 = g[2].x - (g[1].x + g[1].width);
    assert!((gap1 - gap2).abs() < 0.5, "the gaps are {gap1} and {gap2}");
    assert_eq!(g[0].x, 0.0, "the ends stay where they are");
    assert_eq!(g[2].x + g[2].width, 1100.0);
}

#[test]
fn fit_and_cover_put_an_item_over_the_whole_canvas() {
    let s = server();
    two_box(&s);
    s.edit_scene(None, "two", |doc, i| {
        let ids = ops::ids(&doc.scenes[i], &["left".into()])?;
        ops::cover_canvas(doc, i, &ids)
    })
    .expect("covering");
    let g = &s.scene("two").unwrap().geometry[0];
    assert_eq!((g.x, g.y, g.width, g.height), (0.0, 0.0, 1920.0, 1080.0));
}

#[test]
fn arrange_grid_fills_the_canvas_with_no_gaps() {
    let s = server();
    s.edit(None, |doc| {
        let mut scene = Scene::new("wall");
        for n in 1..=4 {
            let mut item = Item::new(Content::Source { source: format!("cam{n}") });
            item.name = Some(format!("cam{n}"));
            scene.items.push(item);
        }
        doc.scenes.push(scene);
        Ok(())
    })
    .unwrap();
    s.edit_scene(None, "wall", |doc, i| {
        let names: Vec<String> = (1..=4).map(|n| format!("cam{n}")).collect();
        let ids = ops::ids(&doc.scenes[i], &names)?;
        ops::arrange_grid(doc, i, &ids, 2)
    })
    .expect("arranging");
    let g = s.scene("wall").unwrap().geometry;
    assert_eq!(g.len(), 4);
    let covered: f64 = g.iter().map(|c| c.width * c.height).sum();
    assert_eq!(covered, 1920.0 * 1080.0, "a two by two grid has to fill the canvas exactly");
    assert_eq!((g[0].x, g[0].y), (0.0, 0.0));
    assert_eq!((g[3].x, g[3].y), (960.0, 540.0));
}

#[test]
fn match_size_leaves_the_one_being_matched_alone() {
    let s = server();
    two_box(&s);
    s.edit_scene(None, "two", |doc, i| {
        let left = find::item_id_in(&doc.scenes[i], "left")?;
        let right = find::item_id_in(&doc.scenes[i], "right")?;
        ops::item_mut(&mut doc.scenes[i].items, right).unwrap().transform.frame =
            Some(Frame::new(200.0, 100.0));
        ops::match_size(doc, i, &[left, right], right)
    })
    .expect("matching");
    let g = s.scene("two").unwrap().geometry;
    assert_eq!((g[0].width, g[0].height), (200.0, 100.0));
    assert_eq!((g[1].width, g[1].height), (200.0, 100.0));
}

/// The OBS issue 2913 case on the document side: grouping changes nothing
/// about the picture, moving the group moves every child, and ungrouping
/// leaves every child where it looked.
#[test]
fn grouping_moving_and_ungrouping_never_corrupts_a_child() {
    let s = server();
    two_box(&s);
    let before = s.scene("two").unwrap().geometry;

    let group_id = s
        .edit(None, |doc| {
            let i = find::scene_index(doc, "two")?;
            let ids = ops::ids(&doc.scenes[i], &["left".into(), "right".into()])?;
            ops::group(doc, i, &ids, Some("pair".into()))
        })
        .expect("grouping")
        .0;
    let grouped = s.scene("two").unwrap().geometry;
    assert_eq!(grouped.len(), 2, "grouping changed how many items are drawn");
    for (was, now) in before.iter().zip(&grouped) {
        assert_eq!((was.x, was.y, was.width, was.height), (now.x, now.y, now.width, now.height));
    }

    s.edit_scene(None, "two", |doc, i| {
        ops::item_mut(&mut doc.scenes[i].items, group_id).unwrap().transform.position =
            Vec2::new(100.0, 0.0);
        Ok(())
    })
    .expect("moving the group");
    let moved = s.scene("two").unwrap().geometry;
    for (was, now) in grouped.iter().zip(&moved) {
        assert_eq!(now.x - was.x, 100.0, "a child did not move with its group");
        assert_eq!((now.y, now.width, now.height), (was.y, was.width, was.height));
    }

    s.edit_scene(None, "two", |doc, i| {
        ops::ungroup(doc, i, group_id).map(|_| ())
    })
    .expect("ungrouping");
    let freed = s.scene("two").unwrap().geometry;
    assert_eq!(freed.len(), 2);
    for (was, now) in moved.iter().zip(&freed) {
        assert_eq!(
            (was.x, was.y, was.width, was.height),
            (now.x, now.y, now.width, now.height),
            "ungrouping moved a child"
        );
    }
}

#[test]
fn ungrouping_something_that_is_not_a_group_says_so_and_changes_nothing() {
    let s = server();
    let view = two_box(&s);
    let item = view.records.iter().find(|r| r.parent.is_some()).unwrap().id;
    let err = s
        .edit_scene(None, "two", |doc, i| ops::ungroup(doc, i, item).map(|_| ()))
        .expect_err("that is not a group");
    assert!(format!("{err}").contains("not a group"), "{err}");
    assert_eq!(s.scene("two").unwrap().geometry.len(), 2, "the failed ungroup lost an item");
}

#[test]
fn reorder_moves_one_item_and_renumbers_nothing() {
    let s = server();
    let view = two_box(&s);
    let orders: Vec<String> = view.records.iter().map(|r| r.order.clone()).collect();
    let left = find::item_id_in(&s.document().scenes[0], "left").unwrap();
    let right = find::item_id_in(&s.document().scenes[0], "right").unwrap();

    let out = s
        .edit_scene(None, "two", |doc, i| ops::reorder(doc, i, right, Some(left), None))
        .expect("moving it to the back");
    let after = s.scene("two").unwrap();
    assert_eq!(after.geometry[0].path, "right", "the stack did not change");
    assert_eq!(orders.len(), after.records.len());
    assert!(!out.patch.updated.is_empty());
}

#[test]
fn create_from_picks_the_layout_by_count_and_names_the_items() {
    let s = server();
    for (count, expected) in [(1usize, "full"), (2, "two-box"), (3, "three-box"), (4, "quad")] {
        assert_eq!(ops::layout_for(count), expected);
    }
    let doc = s.document();
    let scene = ops::create_from(&doc, &["cam1".into(), "cam2".into()], None, None)
        .expect("two sources make a two box");
    assert_eq!(scene.items.len(), 2);
    let names: Vec<Option<String>> = scene.items.iter().map(|i| i.name.clone()).collect();
    assert!(names.iter().all(|n| n.is_some()), "items must be named after their sources: {names:?}");

    // More than four is a grid, whatever the presets have.
    let many: Vec<String> = (1..=6).map(|n| format!("cam{n}")).collect();
    let grid = ops::create_from(&doc, &many, None, None).expect("six sources make a grid");
    assert_eq!(grid.items.len(), 6);
}

#[test]
fn create_from_with_a_layout_too_small_says_how_many_slots_it_has() {
    let s = server();
    let doc = s.document();
    let many: Vec<String> = (1..=5).map(|n| format!("cam{n}")).collect();
    let err = ops::create_from(&doc, &many, Some("two-box"), None).expect_err("two slots, five sources");
    assert!(format!("{err}").contains("slots"), "{err}");
}

#[test]
fn a_layout_copied_from_one_scene_lands_on_another_by_name() {
    let s = server();
    two_box(&s);
    // A second scene with the same item names in the other order.
    s.edit(None, |doc| {
        let mut scene = Scene::new("evening");
        for name in ["right", "left"] {
            let mut item = Item::new(Content::Source { source: "cam9".into() });
            item.name = Some(name.into());
            scene.items.push(item);
        }
        doc.scenes.push(scene);
        Ok(())
    })
    .unwrap();

    let layout = ops::copy_layout(s.document().scene_by_name("two").unwrap());
    let moved = s
        .edit(None, |doc| {
            let i = find::scene_index(doc, "evening")?;
            Ok(ops::paste_layout(&mut doc.scenes[i], &layout, ops::Match::Name))
        })
        .expect("pasting")
        .0;
    assert_eq!(moved, 2);
    let g = s.scene("evening").unwrap().geometry;
    // "right" is first in the evening scene and must have taken the right hand
    // box, not the first one in the layout.
    assert_eq!(g[0].path, "right");
    assert_eq!(g[0].x, 960.0, "matching by name put the wrong geometry on it");
    assert_eq!(g[1].x, 0.0);
}

#[test]
fn a_layout_pasted_by_order_ignores_the_names() {
    let s = server();
    two_box(&s);
    s.edit(None, |doc| {
        let mut scene = Scene::new("evening");
        for name in ["right", "left"] {
            let mut item = Item::new(Content::Source { source: "cam9".into() });
            item.name = Some(name.into());
            scene.items.push(item);
        }
        doc.scenes.push(scene);
        Ok(())
    })
    .unwrap();
    let layout = ops::copy_layout(s.document().scene_by_name("two").unwrap());
    s.edit(None, |doc| {
        let i = find::scene_index(doc, "evening")?;
        Ok(ops::paste_layout(&mut doc.scenes[i], &layout, ops::Match::Order))
    })
    .unwrap();
    let g = s.scene("evening").unwrap().geometry;
    assert_eq!(g[0].path, "right");
    assert_eq!(g[0].x, 0.0, "by order, the first item takes the first box");
}

#[test]
fn an_unmatched_item_is_left_exactly_as_it_was() {
    let s = server();
    two_box(&s);
    s.edit(None, |doc| {
        let mut scene = Scene::new("evening");
        let mut item = Item::new(Content::Source { source: "cam9".into() });
        item.name = Some("nothing like it".into());
        item.transform.position = Vec2::new(7.0, 11.0);
        item.transform.frame = Some(Frame::new(13.0, 17.0));
        scene.items.push(item);
        doc.scenes.push(scene);
        Ok(())
    })
    .unwrap();
    let mut layout = ops::copy_layout(s.document().scene_by_name("two").unwrap());
    layout.items.clear();
    s.edit(None, |doc| {
        let i = find::scene_index(doc, "evening")?;
        Ok(ops::paste_layout(&mut doc.scenes[i], &layout, ops::Match::Name))
    })
    .unwrap();
    let g = &s.scene("evening").unwrap().geometry[0];
    assert_eq!((g.x, g.y, g.width, g.height), (7.0, 11.0, 13.0, 17.0));
}

#[test]
fn a_duplicated_scene_shares_no_id_with_its_original() {
    let s = server();
    two_box(&s);
    let original = s.scene("two").unwrap();
    let copy = s
        .edit(None, |doc| {
            let i = find::scene_index(doc, "two")?;
            let mut copy = doc.scenes[i].clone();
            ops::renumber(&mut copy);
            copy.name = find::free_scene_name(doc, "two");
            let id = copy.id;
            doc.scenes.push(copy);
            Ok(id)
        })
        .expect("duplicating")
        .0;
    let copied = s.scene(&copy.to_string()).unwrap();
    assert_eq!(copied.name, "two 2");
    assert_ne!(copied.id, original.id);
    let originals: Vec<Id> = original.records.iter().map(|r| r.id).collect();
    for record in &copied.records {
        assert!(!originals.contains(&record.id), "the copy shares an id with its original");
    }
}

#[test]
fn a_new_item_with_no_transform_lands_in_the_next_free_cell() {
    let s = server();
    let doc = s.document();
    let empty = Scene::new("empty");
    let first = ops::next_free_cell(&doc, &empty);
    assert_eq!(first.frame, Some(Frame::new(1920.0, 1080.0)), "the first item fills the canvas");
    assert_eq!(first.fit, Fit::Cover);

    two_box(&s);
    let doc = s.document();
    let third = ops::next_free_cell(&doc, doc.scene_by_name("two").unwrap());
    assert!(third.frame.unwrap().w < 1920.0, "the third item has to make room");
}

#[test]
fn validation_reports_what_an_agent_should_fix_before_it_says_done() {
    let s = server();
    s.edit(None, |doc| {
        let mut scene = Scene::new("off");
        let mut item = Item::new(Content::Source { source: "cam1".into() });
        item.name = Some("stray".into());
        item.transform.position = Vec2::new(5000.0, 5000.0);
        item.transform.frame = Some(Frame::new(100.0, 100.0));
        scene.items.push(item);
        doc.scenes.push(scene);
        Ok(())
    })
    .unwrap();
    let findings = s.validate(Some("off")).expect("validating");
    assert!(
        findings.iter().any(|f| f.code.starts_with("scene.off_canvas")),
        "an item off the canvas was not reported: {findings:?}"
    );
    // And the scene view carries them, so a client does not have to ask twice.
    assert!(!s.scene("off").unwrap().findings.is_empty());
}

#[test]
fn a_change_that_would_make_a_scene_contain_itself_is_refused() {
    let s = server();
    two_box(&s);
    let id = s.scene("two").unwrap().id;
    let err = s
        .edit_scene(None, "two", |doc, i| {
            doc.scenes[i]
                .items
                .push(Item::new(Content::Ref { scene: id, overrides: Default::default() }));
            Ok(())
        })
        .expect_err("a scene cannot contain itself");
    assert!(format!("{err:#}").contains("contain itself"), "{err:#}");
    assert_eq!(s.scene("two").unwrap().geometry.len(), 2, "the refused change was kept");
}

// --- the collection's own properties ---------------------------------------

/// `scene.params.set`, `source.set` and `source.group` change nothing that has
/// a record id. Before the patch carried a header, `edit` read the record diff,
/// found it empty, and threw the whole working copy away: all three answered
/// with the change they had made and none of them kept it. This is that bug,
/// as a test.
#[test]
fn a_change_to_the_collections_parameters_is_kept_and_published() {
    let server = server();
    let mut patches = server.subscribe();

    let (_, patch) = server
        .edit(None, |doc| {
            doc.params["properties"]["speaker"] =
                serde_json::json!({ "type": "string", "default": "Ada Lovelace" });
            Ok(())
        })
        .expect("setting a parameter");

    assert!(!patch.is_empty(), "a header change is a change");
    assert_eq!(
        server.document().params["properties"]["speaker"]["default"],
        "Ada Lovelace",
        "the parameter did not survive the edit"
    );
    let published = patches.try_recv().expect("nothing was published");
    assert_eq!(
        published.header.expect("no header on the patch").after.params["properties"]["speaker"]
            ["default"],
        "Ada Lovelace"
    );
}

#[test]
fn a_source_label_is_kept_and_undone() {
    let server = server();
    server
        .edit(None, |doc| {
            doc.sources.insert(
                "cam1".into(),
                crate::scene::document::SourceMeta {
                    name: Some("Wide".into()),
                    color: Some("#ff0000".into()),
                    group: None,
                },
            );
            Ok(())
        })
        .expect("naming a source");
    assert_eq!(server.document().sources["cam1"].name.as_deref(), Some("Wide"));

    server.undo(None).expect("undoing it");
    assert!(server.document().sources.is_empty(), "undo did not take the label off");

    server.redo(None).expect("putting it back");
    assert_eq!(server.document().sources["cam1"].color.as_deref(), Some("#ff0000"));
}

#[test]
fn a_header_change_and_an_item_change_in_one_transaction_are_one_step() {
    let server = server();
    let scene = two_box(&server).name;
    server.begin().expect("a transaction");
    server
        .edit(None, |doc| {
            doc.params["properties"]["speaker"] =
                serde_json::json!({ "type": "string", "default": "Ada" });
            Ok(())
        })
        .unwrap();
    server
        .edit_scene(None, &scene, |doc, i| {
            doc.scenes[i].items.push(Item::new(Content::Source { source: "cam3".into() }));
            Ok(())
        })
        .unwrap();
    let patch = server.commit(None).expect("committing");
    assert!(patch.header.is_some(), "the batch lost the parameter");
    assert_eq!(patch.added.len(), 1);

    server.undo(None).expect("undoing the batch");
    let after = server.document();
    assert!(after.params["properties"].get("speaker").is_none(), "the parameter came back");
    assert_eq!(after.scenes[0].items.len(), 2, "the item came back with it");
}

/// Setting what is already set is still a no op, which is what makes every
/// command idempotent. The header must not turn that into a change.
#[test]
fn setting_a_parameter_to_what_it_already_is_changes_nothing() {
    let server = server();
    server
        .edit(None, |doc| {
            doc.params["properties"]["speaker"] = serde_json::json!({ "default": "Ada" });
            Ok(())
        })
        .unwrap();
    let (_, again) = server
        .edit(None, |doc| {
            doc.params["properties"]["speaker"] = serde_json::json!({ "default": "Ada" });
            Ok(())
        })
        .unwrap();
    assert!(again.is_empty(), "the same value twice is not a change");
}
