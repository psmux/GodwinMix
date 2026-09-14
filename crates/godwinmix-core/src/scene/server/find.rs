//! Finding a scene or an item by whatever the caller typed.
//!
//! Semantic first. An agent reasons about `lower-third` and not about
//! `item_4f2a`, and so does a person at a command line, so every command takes
//! a name as readily as an id and a miss answers with the names that would
//! have worked. That last part is the rule from 03 section 6: every error
//! names the current state and the next step.

use anyhow::{bail, Result};

use crate::scene::document::{Collection, Item, Scene};
use crate::scene::id::Id;

/// Find a scene by id or by name.
pub fn scene<'a>(doc: &'a Collection, which: &str) -> Result<&'a Scene> {
    let index = scene_index(doc, which)?;
    Ok(&doc.scenes[index])
}

/// The same, as a position, for the times the caller needs to change it.
pub fn scene_index(doc: &Collection, which: &str) -> Result<usize> {
    let key = which.trim();
    if let Ok(id) = Id::parse(key) {
        if let Some(pos) = doc.scenes.iter().position(|s| s.id == id) {
            return Ok(pos);
        }
    }
    if let Some(pos) = doc.scenes.iter().position(|s| s.name == key) {
        return Ok(pos);
    }
    // Case folded second, so two scenes differing only in case are still
    // reachable by their exact names.
    let lower = key.to_lowercase();
    if let Some(pos) = doc.scenes.iter().position(|s| s.name.to_lowercase() == lower) {
        return Ok(pos);
    }
    bail!("there is no scene called {key:?}. This collection has: {}", scene_names(doc));
}

/// Every scene name, for an error or a listing.
pub fn scene_names(doc: &Collection) -> String {
    if doc.scenes.is_empty() {
        return "no scenes yet. Make one with scene.add".into();
    }
    doc.scenes.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
}

/// Find an item by id or by name, anywhere in the collection, and say which
/// scene it is in.
pub fn item<'a>(doc: &'a Collection, which: &str) -> Result<(&'a Scene, &'a Item)> {
    let key = which.trim();
    if let Ok(id) = Id::parse(key) {
        for scene in &doc.scenes {
            if let Some(item) = scene.walk().into_iter().find(|i| i.id == id) {
                return Ok((scene, item));
            }
        }
    }
    let mut found = None;
    for scene in &doc.scenes {
        for item in scene.walk() {
            if item.name.as_deref() == Some(key) {
                if found.is_some() {
                    bail!(
                        "more than one item is called {key:?}. Name the scene as well, or use the \
                         item's id: {}",
                        item_names(doc)
                    );
                }
                found = Some((scene, item));
            }
        }
    }
    found.ok_or_else(|| {
        anyhow::anyhow!("there is no item called {key:?}. This collection has: {}", item_names(doc))
    })
}

/// Find an item inside one scene, which is what a command that already knows
/// the scene should use: two scenes may both have a `lower third`.
pub fn item_in<'a>(scene: &'a Scene, which: &str) -> Result<&'a Item> {
    let key = which.trim();
    if let Ok(id) = Id::parse(key) {
        if let Some(item) = scene.walk().into_iter().find(|i| i.id == id) {
            return Ok(item);
        }
    }
    scene
        .walk()
        .into_iter()
        .find(|i| i.name.as_deref() == Some(key))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "the scene {:?} has no item called {key:?}. It has: {}",
                scene.name,
                names_in(scene)
            )
        })
}

/// The id of an item named in one scene.
pub fn item_id_in(scene: &Scene, which: &str) -> Result<Id> {
    item_in(scene, which).map(|i| i.id)
}

/// Every item name in one scene, for an error.
pub fn names_in(scene: &Scene) -> String {
    let names: Vec<String> = scene
        .walk()
        .iter()
        .map(|i| i.name.clone().unwrap_or_else(|| i.id.to_string()))
        .collect();
    if names.is_empty() {
        "nothing yet. Put something in it with scene.item.add".into()
    } else {
        names.join(", ")
    }
}

/// Every item name in the collection, scene qualified.
pub fn item_names(doc: &Collection) -> String {
    let names: Vec<String> = doc
        .scenes
        .iter()
        .flat_map(|s| {
            s.walk()
                .into_iter()
                .map(move |i| format!("{}/{}", s.name, i.name.clone().unwrap_or_else(|| i.id.to_string())))
        })
        .collect();
    if names.is_empty() {
        "nothing yet".into()
    } else {
        names.join(", ")
    }
}

/// A name nothing else in the scene is using, for a duplicate or a copy.
pub fn free_name(scene: &Scene, wanted: &str) -> String {
    let taken: Vec<String> = scene.walk().iter().filter_map(|i| i.name.clone()).collect();
    if !taken.iter().any(|n| n == wanted) {
        return wanted.to_string();
    }
    for n in 2..1000 {
        let candidate = format!("{wanted} {n}");
        if !taken.iter().any(|n| *n == candidate) {
            return candidate;
        }
    }
    format!("{wanted} {}", Id::new())
}

/// A scene name nothing else in the collection is using.
pub fn free_scene_name(doc: &Collection, wanted: &str) -> String {
    if !doc.scenes.iter().any(|s| s.name == wanted) {
        return wanted.to_string();
    }
    for n in 2..1000 {
        let candidate = format!("{wanted} {n}");
        if !doc.scenes.iter().any(|s| s.name == candidate) {
            return candidate;
        }
    }
    format!("{wanted} {}", Id::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::document::{Canvas, Content, Item, Scene};

    fn doc() -> Collection {
        let mut doc = Collection::new("show", Canvas::default());
        let mut wide = Scene::new("Sunday wide");
        let mut item = Item::new(Content::Source { source: "cam1".into() });
        item.name = Some("stage".into());
        wide.items.push(item);
        doc.scenes.push(wide);
        doc.scenes.push(Scene::new("evening"));
        doc
    }

    #[test]
    fn a_scene_answers_to_its_name_its_id_and_its_name_in_any_case() {
        let doc = doc();
        let id = doc.scenes[0].id;
        assert_eq!(scene(&doc, "Sunday wide").unwrap().id, id);
        assert_eq!(scene(&doc, &id.to_string()).unwrap().id, id);
        assert_eq!(scene(&doc, "sunday WIDE").unwrap().id, id);
    }

    #[test]
    fn a_miss_lists_what_would_have_worked() {
        let doc = doc();
        let err = scene(&doc, "nope").expect_err("there is no such scene");
        let text = format!("{err}");
        assert!(text.contains("Sunday wide") && text.contains("evening"), "{text}");
    }

    #[test]
    fn an_item_answers_to_its_name_and_says_where_it_is() {
        let doc = doc();
        let (scene, item) = item(&doc, "stage").expect("the item is there");
        assert_eq!(scene.name, "Sunday wide");
        assert_eq!(item.name.as_deref(), Some("stage"));
        let err = super::item(&doc, "ghost").expect_err("no such item");
        assert!(format!("{err}").contains("Sunday wide/stage"), "{err}");
    }

    #[test]
    fn an_ambiguous_name_says_so_rather_than_picking_one() {
        let mut doc = doc();
        let mut twin = Item::new(Content::Source { source: "cam2".into() });
        twin.name = Some("stage".into());
        doc.scenes[1].items.push(twin);
        let err = item(&doc, "stage").expect_err("two items share the name");
        assert!(format!("{err}").contains("more than one"), "{err}");
    }

    #[test]
    fn a_free_name_does_not_collide() {
        let doc = doc();
        assert_eq!(free_name(&doc.scenes[0], "stage"), "stage 2");
        assert_eq!(free_name(&doc.scenes[0], "corner"), "corner");
        assert_eq!(free_scene_name(&doc, "Sunday wide"), "Sunday wide 2");
    }
}
