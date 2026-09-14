//! Filling a graphic by name, and the three layers a rendered value comes
//! from. The catalogue half is exercised against a real plugin directory by
//! `plugins/ograf` and by `dev/smoke.sh`; what is here needs no plugin and so
//! runs on a bare machine.

use super::*;
use crate::scene::document::{Canvas, Scene};
use serde_json::json;

fn ograf() -> Ograf {
    serde_json::from_value(json!({
        "id": "lower-third",
        "name": "Lower third",
        "main": "graphic.mjs",
        "stepCount": 1,
        "schema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "title": "Name", "default": "" },
                "title": { "type": "string", "title": "Title", "default": "" },
                "colour": { "type": "string", "default": "#1f6f4f" },
                "hold_secs": { "type": "number", "default": 6 }
            }
        },
        "somethingOGrafAddsLater": { "keep": "me" }
    }))
    .expect("the OGraf subset parses")
}

/// A collection with one lower third bound to a collection parameter.
fn doc() -> Collection {
    let mut doc = Collection::new("show", Canvas::default());
    doc.params = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "speaker": { "type": "string", "default": "Ada Lovelace" },
            "hold": { "type": "number", "default": 9 }
        }
    });
    let mut item = Item::new(Content::Graphic {
        graphic: "ograf/lower-third".into(),
        params: json!({ "name": "{{speaker}}", "hold_secs": "{{hold}}" }),
    });
    item.name = Some("lower third".into());
    let mut scene = Scene::new("wide");
    scene.items.push(item);
    doc.scenes.push(scene);
    doc
}

#[test]
fn a_key_ograf_adds_later_survives_a_round_trip() {
    let back = serde_json::to_value(ograf()).unwrap();
    assert_eq!(back["somethingOGrafAddsLater"]["keep"], "me");
    assert_eq!(back["stepCount"], 1);
}

#[test]
fn a_manifest_with_only_the_required_keys_takes_the_defaults() {
    let bare: Ograf = serde_json::from_value(json!({ "id": "x", "name": "X" })).unwrap();
    assert_eq!(bare.step_count, 1, "a graphic with no stepCount plays in and out");
    assert!(bare.supports_real_time, "on air is the default");
    assert_eq!(bare.schema["properties"], json!({}));
}

#[test]
fn the_values_are_the_defaults_then_the_item_then_the_collection_parameters() {
    let doc = doc();
    let item = doc.scenes[0].items[0].clone();
    let values = values(&doc, &item, Some(&ograf()));
    assert_eq!(values["name"], "Ada Lovelace", "a binding takes the parameter's value");
    assert_eq!(values["hold_secs"], json!(9), "a whole binding keeps the parameter's type");
    assert_eq!(values["colour"], "#1f6f4f", "a field nobody set takes the schema's default");
    assert_eq!(values["title"], "", "and an empty default is still a default");
}

#[test]
fn a_binding_nobody_filled_in_is_left_showing_rather_than_blanked() {
    let mut doc = doc();
    doc.params = crate::scene::document::empty_params();
    let item = doc.scenes[0].items[0].clone();
    let values = values(&doc, &item, Some(&ograf()));
    assert_eq!(values["name"], "{{speaker}}");
}

#[test]
fn a_binding_inside_a_sentence_is_substituted_in_place() {
    let mut doc = doc();
    if let Content::Graphic { params, .. } = &mut doc.scenes[0].items[0].content {
        params["title"] = json!("Speaking with {{speaker}} today");
    }
    let item = doc.scenes[0].items[0].clone();
    assert_eq!(
        values(&doc, &item, Some(&ograf()))["title"],
        "Speaking with Ada Lovelace today"
    );
}

#[test]
fn a_field_is_filled_by_name_and_the_records_come_back() {
    let mut doc = doc();
    let fields: Map<String, Value> =
        [("title".to_string(), json!("Analyst"))].into_iter().collect();
    let touched = apply(&mut doc, "ograf/lower-third", &fields, None).unwrap();
    assert_eq!(touched.len(), 1);
    let Content::Graphic { params, .. } = &doc.scenes[0].items[0].content else { panic!() };
    assert_eq!(params["title"], "Analyst");
    assert_eq!(params["name"], "{{speaker}}", "the fields nobody named are left alone");

    let answer = applied(&doc, "ograf/lower-third", &touched);
    assert_eq!(answer.names, vec!["lower third"]);
    assert_eq!(answer.values["title"], "Analyst");
}

#[test]
fn one_placement_of_two_can_be_named() {
    let mut doc = doc();
    let mut second = Item::new(Content::Graphic {
        graphic: "ograf/lower-third".into(),
        params: json!({}),
    });
    second.name = Some("guest strap".into());
    doc.scenes[0].items.push(second);

    let fields: Map<String, Value> = [("name".to_string(), json!("Grace"))].into_iter().collect();
    let touched = apply(&mut doc, "ograf/lower-third", &fields, Some("guest strap")).unwrap();
    assert_eq!(touched.len(), 1);
    let Content::Graphic { params, .. } = &doc.scenes[0].items[1].content else { panic!() };
    assert_eq!(params["name"], "Grace");
    let Content::Graphic { params, .. } = &doc.scenes[0].items[0].content else { panic!() };
    assert_eq!(params["name"], "{{speaker}}", "the other one was not touched");
}

#[test]
fn a_graphic_inside_a_group_is_filled_too() {
    let mut doc = Collection::new("show", Canvas::default());
    let inner = Item::new(Content::Graphic {
        graphic: "ograf/lower-third".into(),
        params: json!({}),
    });
    let id = inner.id;
    let group = Item::new(Content::Children { children: vec![inner] });
    let mut scene = Scene::new("wide");
    scene.items.push(group);
    doc.scenes.push(scene);

    let fields: Map<String, Value> = [("name".to_string(), json!("Ada"))].into_iter().collect();
    assert_eq!(apply(&mut doc, "ograf/lower-third", &fields, None).unwrap(), vec![id]);
}

#[test]
fn filling_a_graphic_nothing_shows_says_how_to_put_one_on_the_canvas() {
    let mut doc = doc();
    let fields: Map<String, Value> = [("name".to_string(), json!("Ada"))].into_iter().collect();
    let err = apply(&mut doc, "ograf/clock", &fields, None).unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("scene.item.add"), "{text}");
    assert!(text.contains("ograf/lower-third"), "the message lists what is there: {text}");
}

#[test]
fn naming_an_item_that_is_not_there_says_so_rather_than_filling_the_others() {
    let mut doc = doc();
    let fields: Map<String, Value> = [("name".to_string(), json!("Ada"))].into_iter().collect();
    let err = apply(&mut doc, "ograf/lower-third", &fields, Some("nonesuch")).unwrap_err();
    assert!(format!("{err:#}").contains("nonesuch"));
    let Content::Graphic { params, .. } = &doc.scenes[0].items[0].content else { panic!() };
    assert_eq!(params["name"], "{{speaker}}", "nothing was written");
}

#[test]
fn the_source_id_is_legible_stable_and_unique_per_placement() {
    let item = Id::new();
    let one = source_id("ograf/lower-third", &item);
    assert!(one.starts_with("graphic-lower-third-"), "{one}");
    assert_eq!(one, source_id("ograf/lower-third", &item), "the same item gives the same id");
    // Ten in a row, because a UUIDv7's leading bytes are the millisecond and
    // the first version of this took them: every id here is minted inside one.
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..10 {
        assert!(seen.insert(source_id("ograf/lower-third", &Id::new())), "two placements, one source");
    }
    assert!(!seen.contains(&one));
    assert!(
        one.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "a source id is a slug an operator types: {one}"
    );
}

#[test]
fn the_page_address_carries_the_instance_the_host_serves() {
    let item = Id::new();
    let url = page_url("http://127.0.0.1:7841/", "ograf/lower-third", &item);
    assert!(url.starts_with("http://127.0.0.1:7841/graphic/ograf/lower-third?instance="), "{url}");
    assert!(url.ends_with(&source_id("ograf/lower-third", &item)), "{url}");
}

#[test]
fn every_graphic_in_the_document_is_found_with_the_source_it_resolves_to() {
    let doc = doc();
    let found = placements(&doc);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1, "ograf/lower-third");
    assert_eq!(found[0].2, source_id("ograf/lower-third", &doc.scenes[0].items[0].id));
}

#[test]
fn a_graphic_this_core_does_not_have_says_how_to_get_one() {
    let err = find("nobody/nothing").unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("gmx plugin new --kind graphic") || text.contains("This core has:"), "{text}");
}
