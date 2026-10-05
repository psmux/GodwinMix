//! Importing transitions and effects: the starter set measures as its own
//! manifests say, a clip with alpha is found to be a stinger, a zip made by
//! an ordinary zip tool unpacks with its licence, a shader is checked, and
//! every import has a preview strip.

use godwinmix_core::fx::{detect, import, library, sprite, starter};
use godwinmix_protocol::fx::{FxBlend, FxImportRequest, FxKind};
use gstreamer as gst;
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-fx-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_starter_clips_measure_as_their_manifests_say() {
    gst::init().unwrap();
    for (name, file, manifest, _) in starter::STARTER {
        if file.ends_with(".glsl") {
            continue;
        }
        let m: godwinmix_protocol::fx::FxManifest = serde_json::from_slice(manifest).unwrap();
        let measured = detect::measure(&repo().join("fx").join(name).join(file)).expect("the starter file decodes");
        let v = detect::classify(&measured);
        println!("{name:<12} {:?} {:?} cut {:?} coverage {:?} transition {} alpha {} {} ms", v.kind, v.blend, v.cut_at_ms, v.coverage, v.transition, measured.alpha, measured.duration_ms);
        assert_eq!(v.kind, m.kind, "{name} measured as the wrong kind");
        if m.kind != FxKind::Matte {
            assert_eq!(v.blend == FxBlend::Normal, m.blend == FxBlend::Normal, "{name}: alpha or light");
            assert!(measured.duration_ms.abs_diff(m.duration_ms) <= 70, "{name}: {} ms, the manifest says {}", measured.duration_ms, m.duration_ms);
        }
        if let (Some(a), Some(b)) = (v.cut_at_ms, m.cut_at_measured_ms) {
            assert!(a.abs_diff(b) <= 100, "{name}: cut measured at {a} ms, the manifest says {b}");
        }
        assert_eq!(v.transition, m.transition, "{name}: whether it covers enough to cut under");
    }
}

#[test]
fn a_clip_with_alpha_imports_as_a_stinger_with_a_preview() {
    gst::init().unwrap();
    let media = scratch("alpha");
    let req = FxImportRequest { path: repo().join("fx/glitch/glitch.webm").display().to_string(), name: Some("My Glitch".into()), ..Default::default() };
    let done = import::import(&media, &req).expect("import");
    let item = &done.imported[0];
    assert_eq!(item.manifest.name, "my-glitch");
    assert_eq!(item.manifest.kind, FxKind::Stinger);
    assert!(sprite::path(Path::new(&item.dir)).is_file(), "an import leaves a preview strip");
    let again = import::import(&media, &req).unwrap_err();
    assert!(again.to_string().contains("replace: true"), "a second import names the way out: {again}");
}

#[test]
fn a_zipped_pack_imports_every_item_with_its_licence() {
    gst::init().unwrap();
    let media = scratch("zip");
    let req = FxImportRequest { path: repo().join("crates/godwinmix-core/tests/fixtures/fx-pack.zip").display().to_string(), ..Default::default() };
    let done = import::import(&media, &req).expect("the zip imports");
    let names: Vec<&str> = done.imported.iter().map(|e| e.manifest.name.as_str()).collect();
    println!("imported {names:?}, skipped {:?}", done.skipped);
    assert!(names.contains(&"pack-burn") && names.contains(&"pack-wipe"), "{names:?}");
    let burn = done.imported.iter().find(|e| e.manifest.name == "pack-burn").unwrap();
    assert_eq!(burn.manifest.kind, FxKind::Overlay);
    assert!(burn.manifest.licence.as_deref().is_some_and(|l| l.contains("CC0")), "the licence travels: {:?}", burn.manifest.licence);
    let (all, _) = library::list(&media);
    assert!(all.len() >= starter::STARTER.len() + 2, "the starter set and the pack are both in the library");
}

#[test]
fn a_shader_with_a_uniform_and_no_default_is_refused_by_name() {
    let media = scratch("shader");
    let file = media.join("speedy.glsl");
    std::fs::write(&file, "uniform float speed;\nvec4 transition(vec2 uv) { return getToColor(uv); }\n").unwrap();
    let req = FxImportRequest { path: file.display().to_string(), ..Default::default() };
    let e = import::import(&media, &req).unwrap_err();
    assert!(format!("{e:#}").contains("speed has no default"), "{e:#}");
    std::fs::write(&file, "uniform float speed; // = 2.0\nvec4 transition(vec2 uv) { return getToColor(uv); }\n").unwrap();
    let done = import::import(&media, &req).expect("with a default it imports");
    assert_eq!(done.imported[0].manifest.kind, FxKind::Shader);
}
