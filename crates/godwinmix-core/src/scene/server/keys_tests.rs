//! Downstream keys on the document: drawn above every take until taken off.

use super::*;
use crate::scene::document::{Content, Frame, Item, Scene};

fn caps() -> CanvasCaps {
    CanvasCaps::new(&crate::config::Canvas { width: 1920, height: 1080, fps: crate::config::FrameRate::whole(30), sample_rate: 48000, channels: 2 })
}

/// A full canvas scene of one source, and a lower third scene of another.
fn show(s: &SceneServer) {
    s.edit(None, |doc| {
        let mut wide = Scene::new("wide");
        wide.items.push(Item::new(Content::Source { source: "cam1".into() }));
        let mut third = Scene::new("lower third");
        let mut bar = Item::new(Content::Source { source: "name-bar".into() });
        bar.transform.frame = Some(Frame::new(1920.0, 200.0));
        third.items.push(bar);
        doc.scenes.push(wide);
        doc.scenes.push(third);
        Ok(())
    })
    .expect("adding the scenes");
}

fn sources(placements: &[Placement]) -> Vec<String> {
    placements.iter().map(|p| p.source.clone()).collect()
}

#[test]
fn a_key_is_drawn_above_every_take_until_it_is_off() {
    let s = SceneServer::in_memory(caps());
    show(&s);
    assert_eq!(sources(&s.placements("wide").unwrap().1), ["cam1"]);

    let on = s.set_key("lower third", true).unwrap();
    assert_eq!(on.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(), ["lower third"]);
    assert_eq!(sources(&s.placements("wide").unwrap().1), ["cam1", "name-bar"], "the key is above the scene");
    assert_eq!(sources(&s.key_placements()), ["name-bar"], "and above a source taken on its own");
    assert_eq!(s.key_sources(), ["name-bar"]);

    // Taking the key's own scene does not draw it twice.
    assert_eq!(sources(&s.placements("lower third").unwrap().1), ["name-bar"]);

    assert!(s.set_key("lower third", false).unwrap().is_empty());
    assert_eq!(sources(&s.placements("wide").unwrap().1), ["cam1"]);
}

#[test]
fn putting_on_a_key_twice_keeps_one_and_an_unknown_scene_says_so() {
    let s = SceneServer::in_memory(caps());
    show(&s);
    s.set_key("lower third", true).unwrap();
    assert_eq!(s.set_key("lower third", true).unwrap().len(), 1);
    let e = s.set_key("no such scene", true).unwrap_err().to_string();
    assert!(e.contains("no such scene"), "{e}");
}

#[test]
fn a_key_survives_an_edit_and_a_removed_scene_leaves_nothing_behind() {
    let s = SceneServer::in_memory(caps());
    show(&s);
    s.set_key("lower third", true).unwrap();
    // Any edit goes through the flat form and back; the key must come with it.
    s.edit(None, |doc| {
        doc.scenes[0].name = "wide shot".into();
        Ok(())
    })
    .unwrap();
    assert_eq!(s.keys().len(), 1, "an edit kept the key");
    s.edit(None, |doc| {
        doc.scenes.retain(|sc| sc.name != "lower third");
        Ok(())
    })
    .unwrap();
    assert!(s.keys().is_empty(), "a key whose scene is gone is not listed");
    assert_eq!(sources(&s.placements("wide shot").unwrap().1), ["cam1"]);
}
