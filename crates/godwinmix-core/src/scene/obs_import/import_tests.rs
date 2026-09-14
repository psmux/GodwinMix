//! Importing the two hand written collections under `tests/fixtures/obs/`.
//!
//! Every expected position in here was worked out from the OBS rules by hand
//! and written down as a number, not read back out of the importer. That is the
//! only way this test can catch the importer being wrong.

use super::*;
use crate::scene::geometry::flatten;
use crate::scene::validate;

const SIMPLE_RAW: &str = include_str!("../../../../../tests/fixtures/obs/simple.json");
const FULL_RAW: &str = include_str!("../../../../../tests/fixtures/obs/full.json");

/// The fixtures with Unix line endings whatever git checked them out as, so the
/// tests that edit the text by hand match on Windows too.
fn fixture(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// A fixture as text. `.gitattributes` pins these files to LF as well; this is
/// the belt to that pair of braces.
fn simple() -> String {
    fixture(SIMPLE_RAW)
}

fn full() -> String {
    fixture(FULL_RAW)
}

fn import_full(options: &Options) -> Import {
    import(&full(), options).expect("the full fixture imports")
}

/// Every placement of a scene as `(path, x, y, w, h)`, rounded.
fn placements(scene: &Scene, canvas: &Canvas) -> Vec<(String, i64, i64, i64, i64)> {
    flatten(&scene.items, canvas)
        .iter()
        .map(|p| {
            let (x, y, w, h) = p.rect.rounded();
            (p.path.clone(), x, y, w, h)
        })
        .collect()
}

fn outcome_of<'a>(report: &'a Report, name: &str) -> &'a Outcome {
    &report
        .sources
        .iter()
        .find(|s| s.obs_name == name)
        .unwrap_or_else(|| panic!("{name} is not in the report"))
        .outcome
}

#[test]
fn the_simple_collection_lands_every_item_where_obs_had_it() {
    let imported = import(&simple(), &Options::default()).unwrap();
    let canvas = Canvas::default();
    assert_eq!(imported.document.scenes.len(), 1);
    let scene = &imported.document.scenes[0];
    assert_eq!(scene.name, "Main");
    assert_eq!(
        placements(scene, &canvas),
        vec![
            // A colour source is 1920x1080 by its own settings, at the origin.
            ("Backdrop".into(), 0, 0, 1920, 1080),
            // Centre aligned at the middle of the canvas, half size: nothing in
            // the file says how big the clip is, so the canvas stands in and
            // 1920x1080 at 0.5 is 960x540, centred on 960,540.
            ("Clip".into(), 480, 270, 960, 540),
            // Bottom right aligned at the bottom right corner, fitted inside a
            // 480x270 bounds box.
            ("Camera".into(), 1440, 810, 480, 270),
        ]
    );
}

#[test]
fn the_simple_collection_reports_what_each_source_became() {
    let imported = import(&simple(), &Options::default()).unwrap();
    assert!(matches!(
        outcome_of(&imported.report, "Backdrop"),
        Outcome::Imported { r#type, id } if r#type == "test/source" && id == "backdrop"
    ));
    assert!(matches!(
        outcome_of(&imported.report, "Clip"),
        Outcome::Imported { r#type, .. } if r#type == "file/source"
    ));
    assert!(matches!(
        outcome_of(&imported.report, "Camera"),
        Outcome::NeedsPlugin { r#type, plugin, .. } if r#type == "camera/source" && plugin == "camera"
    ));
    assert_eq!(imported.report.items, 3);
    assert_eq!(imported.sources.len(), 3);
}

#[test]
fn the_full_collection_lands_every_item_where_obs_had_it() {
    let imported = import_full(&Options::default());
    let canvas = Canvas::default();
    let main = imported
        .document
        .scene_by_name("Main")
        .expect("a scene called Main");
    assert_eq!(
        placements(main, &canvas),
        vec![
            // A colour source at its own size.
            ("Backdrop".into(), 0, 0, 1920, 1080),
            // OBS_BOUNDS_STRETCH into 960x540 at the origin.
            ("CAM 1 (Studio)".into(), 0, 0, 960, 540),
            // The group sits at 1400,40, so its first child, which is at 0,0
            // inside the group and fitted to a 480x270 bounds box by height,
            // lands at 1400,40.
            ("Corner / CAM 2".into(), 1400, 40, 480, 270),
            // The second child is 280 down inside the group: 40 + 280 = 320.
            ("Corner / Scoreboard".into(), 1400, 320, 480, 120),
            // The nested scene, fitted inside 900x140 at 96,880.
            ("Lower third scene".into(), 96, 880, 900, 140),
            // Bottom left aligned at the bottom left corner, 400x225 bounds:
            // 1080 - 225 = 855.
            ("Stream".into(), 0, 855, 400, 225),
            // A text source at a fifth of the canvas, placed at 100,100.
            ("Lower third".into(), 100, 100, 384, 216),
        ]
    );
    // The hidden item is still in the document, it is just not placed.
    let hidden = main
        .items
        .iter()
        .find(|i| i.name.as_deref() == Some("Ad break"))
        .unwrap();
    assert!(!hidden.visible);
    assert_eq!(hidden.transform.frame, Some(Frame::new(640.0, 360.0)));
    assert_eq!(hidden.transform.fit, Fit::Cover);
    assert_eq!(hidden.transform.anchor, Vec2::new(0.5, 0.5));
}

#[test]
fn the_seven_obs_bounds_types_each_arrive_as_a_frame_and_a_fit() {
    let imported = import_full(&Options::default());
    // Keyed by scene as well as by name, because the camera is placed twice
    // and the two placements have different bounds.
    let mut seen: BTreeMap<String, (Fit, Option<Frame>)> = BTreeMap::new();
    for scene in &imported.document.scenes {
        for item in scene.walk() {
            let name = item.name.clone().unwrap_or_default();
            seen.insert(
                format!("{} / {name}", scene.name),
                (item.transform.fit, item.transform.frame),
            );
        }
    }
    // One item per bounds type, in the order of the OBS enum.
    assert_eq!(
        seen["Main / Backdrop"],
        (Fit::Stretch, Some(Frame::new(1920.0, 1080.0)))
    ); // NONE, size known
    assert_eq!(
        seen["Main / CAM 1 (Studio)"],
        (Fit::Stretch, Some(Frame::new(960.0, 540.0)))
    ); // STRETCH
    assert_eq!(
        seen["Main / Lower third scene"],
        (Fit::Contain, Some(Frame::new(900.0, 140.0)))
    ); // SCALE_INNER
    assert_eq!(
        seen["Main / Ad break"],
        (Fit::Cover, Some(Frame::new(640.0, 360.0)))
    ); // SCALE_OUTER
    assert_eq!(
        seen["Main / Stream"],
        (Fit::FitWidth, Some(Frame::new(400.0, 225.0)))
    ); // SCALE_TO_WIDTH
    assert_eq!(
        seen["Main / CAM 2"],
        (Fit::FitHeight, Some(Frame::new(480.0, 270.0)))
    ); // SCALE_TO_HEIGHT
    assert_eq!(
        seen["Main / Scoreboard"],
        (Fit::Max, Some(Frame::new(480.0, 120.0)))
    ); // MAX_ONLY
       // A source whose size nothing declares keeps its scale factors instead.
    assert_eq!(seen["Main / Lower third"], (Fit::None, None));
}

#[test]
fn a_group_becomes_an_item_with_children_whose_transforms_are_already_flat() {
    let imported = import_full(&Options::default());
    let main = imported.document.scene_by_name("Main").unwrap();
    let group = main
        .items
        .iter()
        .find(|i| i.name.as_deref() == Some("Corner"))
        .unwrap();
    let Content::Children { children } = &group.content else {
        panic!(
            "the group did not come across as children: {:?}",
            group.content
        );
    };
    assert_eq!(children.len(), 2);
    // The group's own transform is spent: the children carry the numbers, so
    // nothing has to invert a composition later. That is the whole of OBS's
    // group transform bug class.
    assert_eq!(group.transform, Transform::default());
    assert_eq!(children[0].transform.position, Vec2::new(1400.0, 40.0));
    assert_eq!(children[1].transform.position, Vec2::new(1400.0, 320.0));
    assert_eq!(children[1].blend, Blend::Multiply);
}

#[test]
fn a_nested_scene_becomes_a_reference_to_that_scene() {
    let imported = import_full(&Options::default());
    let main = imported.document.scene_by_name("Main").unwrap();
    let nested = imported
        .document
        .scene_by_name("Lower third scene")
        .unwrap();
    let item = main
        .items
        .iter()
        .find(|i| i.name.as_deref() == Some("Lower third scene"))
        .unwrap();
    match &item.content {
        Content::Ref { scene, overrides } => {
            assert_eq!(*scene, nested.id);
            assert!(overrides.is_empty());
        }
        other => panic!("the nested scene came across as {other:?}"),
    }
    imported.document.check_refs().expect("no cycles");
}

#[test]
fn pixel_crops_become_fractions_of_the_source() {
    // With nothing to go on, the crop is measured against the canvas and the
    // report says so.
    let imported = import_full(&Options::default());
    let main = imported.document.scene_by_name("Main").unwrap();
    let cam = main
        .items
        .iter()
        .find(|i| i.name.as_deref() == Some("CAM 1 (Studio)"))
        .unwrap();
    assert!(
        (cam.crop.left - 160.0 / 1920.0).abs() < 1e-9,
        "{:?}",
        cam.crop
    );
    assert!((cam.crop.right - 160.0 / 1920.0).abs() < 1e-9);
    assert_eq!(cam.crop.top, 0.0);
    assert!(
        imported
            .report
            .notes
            .iter()
            .any(|n| n.contains("--source-size")),
        "{:#?}",
        imported.report.notes
    );

    // Told how big the camera is, it is exact.
    let options = Options {
        source_sizes: BTreeMap::from([("CAM 1 (Studio)".to_string(), (1280.0, 720.0))]),
        ..Options::default()
    };
    let imported = import_full(&options);
    let main = imported.document.scene_by_name("Main").unwrap();
    let cam = main
        .items
        .iter()
        .find(|i| i.name.as_deref() == Some("CAM 1 (Studio)"))
        .unwrap();
    assert!((cam.crop.left - 0.125).abs() < 1e-9, "{:?}", cam.crop);
}

#[test]
fn a_source_filter_is_copied_onto_every_placement_and_the_report_names_them() {
    let imported = import_full(&Options::default());
    let mut carrying = Vec::new();
    for scene in &imported.document.scenes {
        for item in scene.walk() {
            if item
                .filters
                .iter()
                .any(|f| f.name.as_deref() == Some("Key"))
            {
                carrying.push(format!(
                    "{} / {}",
                    scene.name,
                    item.name.clone().unwrap_or_default()
                ));
            }
        }
    }
    assert_eq!(
        carrying,
        vec![
            "Main / CAM 1 (Studio)",
            "Lower third scene / CAM 1 (Studio)"
        ],
        "the camera appears in two scenes, so its key has to be on both items"
    );

    let report = &imported.report.filters_duplicated;
    assert_eq!(report.len(), 1, "{report:#?}");
    assert_eq!(report[0].filter, "Key");
    assert_eq!(report[0].source, "CAM 1 (Studio)");
    assert_eq!(report[0].obs_type, "chroma_key_filter_v2");
    assert_eq!(report[0].placements.len(), 2);

    // The filter's own settings came across untouched.
    let main = imported.document.scene_by_name("Main").unwrap();
    let cam = main
        .items
        .iter()
        .find(|i| i.name.as_deref() == Some("CAM 1 (Studio)"))
        .unwrap();
    assert_eq!(cam.filters[0].kind, "chroma/filter");
    assert_eq!(cam.filters[0].params["similarity"], json!(400));
    assert!(cam.filters[0].enabled);
}

#[test]
fn the_report_says_what_happened_to_every_source() {
    let imported = import_full(&Options::default());
    let r = &imported.report;
    assert_eq!(
        r.sources.len(),
        11,
        "one line per OBS source, scenes and groups included"
    );
    assert!(matches!(
        outcome_of(r, "Scoreboard"),
        Outcome::Imported { r#type, id } if r#type == "browser/source" && id == "scoreboard"
    ));
    assert!(matches!(
        outcome_of(r, "Stream"),
        Outcome::Imported { r#type, .. } if r#type == "rtmp/source"
    ));
    assert!(matches!(
        outcome_of(r, "Ad break"),
        Outcome::Imported { r#type, .. } if r#type == "file/source"
    ));
    assert!(matches!(
        outcome_of(r, "CAM 2"),
        Outcome::NeedsPlugin { plugin, .. } if plugin == "camera"
    ));
    match outcome_of(r, "Old plugin") {
        Outcome::Skipped {
            reason,
            placeholder,
        } => {
            assert!(reason.contains("obs_wobbler_source"), "{reason}");
            assert!(placeholder.is_none());
        }
        other => panic!("an unknown OBS type should be skipped, not {other:?}"),
    }
    match outcome_of(r, "Lower third") {
        Outcome::Skipped { placeholder, .. } => {
            assert!(placeholder.as_deref().unwrap().contains("text/graphic"));
        }
        other => panic!("a text source should be skipped with a placeholder, not {other:?}"),
    }
    // A source nobody could play leaves no item behind.
    let main = imported.document.scene_by_name("Main").unwrap();
    assert!(!main
        .items
        .iter()
        .any(|i| i.name.as_deref() == Some("Old plugin")));
    // Counting placements is what tells an operator which source matters.
    let cam1 = r
        .sources
        .iter()
        .find(|s| s.obs_name == "CAM 1 (Studio)")
        .unwrap();
    assert_eq!(cam1.placements, 2);
}

#[test]
fn the_source_list_comes_out_as_config_with_a_type_and_params() {
    let imported = import_full(&Options::default());
    let toml_text = imported.to_config_toml().unwrap();
    let parsed: toml::Value = toml::from_str(&toml_text).expect("the config parses as TOML");
    let sources = parsed["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 6, "{toml_text}");
    let by_id = |id: &str| {
        sources
            .iter()
            .find(|s| s["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("{id} is not in the config:\n{toml_text}"))
            .clone()
    };
    let browser = by_id("scoreboard");
    assert_eq!(browser["type"].as_str(), Some("browser/source"));
    assert_eq!(
        browser["params"]["url"].as_str(),
        Some("https://example.com/score")
    );
    assert_eq!(
        browser["uri"].as_str(),
        Some("web+https://example.com/score")
    );
    let stream = by_id("stream");
    assert_eq!(stream["type"].as_str(), Some("rtmp/source"));
    assert_eq!(
        stream["uri"].as_str(),
        Some("rtmp://ingest.example.com/live/guest")
    );
    let camera = by_id("cam-1-studio");
    assert_eq!(camera["type"].as_str(), Some("camera/source"));
    assert_eq!(camera["name"].as_str(), Some("CAM 1 (Studio)"));
    assert_eq!(camera["params"]["device"].as_str(), Some("/dev/video0"));
    let colour = by_id("backdrop");
    assert!(colour["params"]["color"].as_str().unwrap().starts_with('#'));
}

#[test]
fn the_imported_document_round_trips_and_validates() {
    let imported = import_full(&Options::default());
    let text = imported.document.to_json();
    let back = Collection::from_json(&text).expect("the written document reads back");
    assert_eq!(back, imported.document);
    assert_eq!(back.to_flat().to_tree().unwrap(), imported.document);
    let findings = validate::collection(&imported.document);
    assert!(!validate::has_errors(&findings), "{findings:#?}");
}

#[test]
fn importing_onto_a_smaller_canvas_keeps_the_crops_and_says_which_canvas_it_used() {
    let options = Options {
        canvas: Some(Canvas::parse("1280x720").unwrap()),
        ..Options::default()
    };
    let imported = import_full(&options);
    assert_eq!(imported.report.canvas.width, 1280);
    // A normalised crop is the same fraction whatever the canvas is; here it is
    // measured against the canvas because nothing says how big the camera is.
    let main = imported.document.scene_by_name("Main").unwrap();
    let cam = main
        .items
        .iter()
        .find(|i| i.name.as_deref() == Some("CAM 1 (Studio)"))
        .unwrap();
    assert!(
        (cam.crop.left - 160.0 / 1280.0).abs() < 1e-9,
        "{:?}",
        cam.crop
    );
    assert!(
        !imported
            .report
            .notes
            .iter()
            .any(|n| n.contains("assumed 1920x1080")),
        "the canvas was given, so it should not be reported as assumed"
    );
}

#[test]
fn scenes_come_out_in_the_order_obs_lists_them() {
    let imported = import_full(&Options::default());
    let names: Vec<&str> = imported
        .document
        .scenes
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, vec!["Main", "Lower third scene"]);
}

#[test]
fn a_file_that_is_not_a_collection_says_where_to_get_one() {
    let err = import("{\"sources\": 3}", &Options::default()).unwrap_err();
    assert!(format!("{err:#}").contains("Scene Collection"), "{err:#}");
}

#[test]
fn an_item_naming_a_source_that_is_gone_is_reported_rather_than_dropped_in_silence() {
    // Both the uuid and the name have to be wrong: OBS files address by uuid
    // with a fallback to the name, and the importer tries both.
    let text = simple().replace(
        "\"name\": \"Camera\",\n            \"source_uuid\": \"33333333-3333-3333-3333-333333333333\"",
        "\"name\": \"Ghost\",\n            \"source_uuid\": \"deadbeef-0000-0000-0000-000000000000\"",
    );
    let imported = import(&text, &Options::default()).unwrap();
    assert!(
        imported
            .report
            .notes
            .iter()
            .any(|n| n.contains("not in this collection")),
        "{:#?}",
        imported.report.notes
    );
}

#[test]
fn an_item_falls_back_to_the_source_name_when_the_uuid_is_gone() {
    // OBS wrote names before it wrote uuids, and the current serialiser still
    // falls back to the name, so a file with no uuids has to import.
    let text = simple().replace("source_uuid", "was_source_uuid");
    let imported = import(&text, &Options::default()).unwrap();
    assert_eq!(imported.document.scenes[0].items.len(), 3);
    assert!(imported
        .report
        .notes
        .iter()
        .all(|n| !n.contains("not in this collection")));
}

#[test]
fn a_group_that_contains_itself_is_reported_and_not_recursed_into() {
    let text = full().replace(
        "\"name\": \"CAM 2\",\n            \"source_uuid\": \"a0000000-0000-0000-0000-000000000003\"",
        "\"name\": \"Corner\",\n            \"source_uuid\": \"a0000000-0000-0000-0000-000000000009\"",
    );
    let imported = import(&text, &Options::default()).expect("it imports rather than looping");
    assert!(
        imported
            .report
            .notes
            .iter()
            .any(|n| n.contains("contains itself")),
        "{:#?}",
        imported.report.notes
    );
}
