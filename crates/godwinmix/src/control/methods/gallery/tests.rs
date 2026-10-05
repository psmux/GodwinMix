//! The gallery's tools as an agent finds them, and its forgiving inputs.

use crate::control::methods::registry;
use godwinmix_protocol::gallery::GallerySaveRequest;
use godwinmix_protocol::mcp_tools::{all_tools, search};

/// What a model types into search_tools when asked for a graphic, and a
/// gallery tool it should find among the first three.
const ASKS: &[(&str, &str)] = &[
    ("make a lower third", "save_graphic"),
    ("save a graphic I designed", "save_graphic"),
    ("graphics gallery", "list_graphics"),
    ("look at my graphic", "preview_graphic"),
    ("place a graphic on the scene", "place_graphic"),
    ("show the graphic on air", "show_graphic"),
    ("import graphic files", "import_graphics"),
    ("save a virtual set", "save_graphic"),
];

#[test]
fn a_model_finds_the_gallery_tools_by_asking_in_plain_words() {
    let reg = registry();
    for (ask, tool) in ASKS {
        let found: Vec<String> = search(&reg, ask, 3).iter().map(|t| t["name"].as_str().unwrap_or_default().to_string()).collect();
        assert!(found.iter().any(|f| f == tool), "{ask:?} found {found:?}, not {tool}");
    }
}

/// The skill and the docs name the tools; a renamed tool fails here rather
/// than in an agent's session.
#[test]
fn every_gallery_tool_the_skill_and_the_how_to_name_exists() {
    let names: Vec<String> = all_tools(&registry()).iter().map(|t| t["name"].as_str().unwrap_or_default().to_string()).collect();
    for text in [
        include_str!("../../../../../../skills/godwinmix-design/SKILL.md"),
        include_str!("../../../../../../docs/how-to/build-a-graphics-gallery-with-ai.md"),
        include_str!("../../../../../../ui/panels/graphics/prompts.js"),
    ] {
        for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if word.ends_with("_graphic") || word.ends_with("_graphics") {
                assert!(names.iter().any(|n| n == word), "{word} is named and is not a tool");
            }
        }
    }
}

#[test]
fn every_gallery_tool_is_short_enough_to_read_and_names_no_dashes() {
    let tools = all_tools(&registry());
    let ours: Vec<_> = tools.iter().filter(|t| t["method"].as_str().is_some_and(|m| m.starts_with("gallery."))).collect();
    assert_eq!(ours.len(), 10, "ten gallery tools");
    for t in ours {
        let d = t["description"].as_str().unwrap_or_default();
        assert!(d.len() < 800, "{} has a {} byte description; a small model reads every byte", t["name"], d.len());
        assert!(!d.contains('\u{2014}') && !d.contains('\u{2013}'), "{} has a dash", t["name"]);
    }
}

#[test]
fn a_save_reads_the_ways_models_write_it() {
    let r: GallerySaveRequest = serde_json::from_value(serde_json::json!({
        "title": "Storm", "path": "C:/x.png", "fields": {"headline": "Hi"}, "tags": "news, red", "position": "l3", "overwrite": true
    }))
    .expect("aliases are read");
    assert_eq!(r.name, "Storm");
    assert_eq!(r.file.as_deref(), Some("C:/x.png"));
    assert!(r.replace && r.values.is_some());
    assert_eq!(super::words(r.tags.as_ref()), ["news", "red"]);
    assert_eq!(godwinmix_protocol::gallery::Zone::parse(r.zone.as_deref().unwrap()), Some(godwinmix_protocol::gallery::Zone::LowerThird));
    let r: GallerySaveRequest = serde_json::from_value(serde_json::json!({"name": "x", "svg_code": "<svg/>"})).expect("svg_code is read as svg");
    assert_eq!(r.svg.as_deref(), Some("<svg/>"));
    let wrong = serde_json::from_value::<GallerySaveRequest>(serde_json::json!({"name": "x", "svg_markup": "<svg/>"})).unwrap_err().to_string();
    assert!(wrong.contains("svg") && wrong.contains("expected one of"), "an unknown field names the ones there are: {wrong}");
    let untitled: GallerySaveRequest = serde_json::from_value(serde_json::json!({"svg": "<svg/>"})).expect("a missing name reaches the handler, which says what to do");
    assert!(untitled.name.is_empty());
}

#[test]
fn base64_arrives_plain_or_as_a_data_uri() {
    let (ext, bytes) = super::save::decode("data:image/png;base64,iVBORw0KGgo=").unwrap();
    assert_eq!(ext, Some("png"));
    assert!(bytes.starts_with(b"\x89PNG"));
    let (ext, bytes) = super::save::decode("aGVs\nbG8").unwrap();
    assert_eq!((ext, bytes.as_slice()), (None, &b"hello"[..]));
    let bad = super::save::decode("not base64 at all!").unwrap_err();
    assert!(bad.message.contains("data:image/png;base64"), "{}", bad.message);
}
