//! Export then import has to give back what went in, and an asset that is not
//! there has to be named rather than guessed at. Those are the two things a
//! share is for.

use super::*;
use crate::scene::document::{Canvas, Filter, Frame, Item, Scene};
use serde_json::json;

/// A scratch directory that cleans up after itself.
struct Dir(PathBuf);

impl Dir {
    fn new(what: &str) -> Dir {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let at = std::env::temp_dir().join(format!("gmx-collection-{what}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&at);
        std::fs::create_dir_all(&at).expect("a scratch directory");
        Dir(at)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const LOGO: &[u8] = b"\x89PNG\r\n\x1a\n not really a png, but it hashes";

/// A collection with a graphic, a filter and an asset: one of everything a
/// share has to carry.
fn sample(root: &Path) -> Collection {
    std::fs::create_dir_all(root.join("media")).unwrap();
    std::fs::write(root.join("media/logo.png"), LOGO).unwrap();

    let mut doc = Collection::new("Sunday service", Canvas::default());
    let id = Id::new();
    doc.assets.insert(
        id,
        Asset {
            path: "media/logo.png".into(),
            sha256: Some(godwinmix_host::verify::sha256::hex(LOGO)),
            size: Some(LOGO.len() as u64),
        },
    );

    let mut camera = Item::new(Content::Source { source: "cam-wide".into() });
    camera.name = Some("stage".into());
    camera.filters.push(Filter {
        kind: "chroma/filter".into(),
        name: None,
        enabled: true,
        params: json!({ "method": "green" }),
    });

    let mut lower = Item::new(Content::Graphic {
        graphic: "ograf/lower-third".into(),
        params: json!({ "name": "Ada Lovelace", "title": "Analyst", "badge": id.to_string() }),
    });
    lower.name = Some("lower third".into());
    lower.transform.frame = Some(Frame::new(960.0, 180.0));

    let mut scene = Scene::new("Wide with lower third");
    scene.items = vec![camera, lower];
    doc.scenes.push(scene);
    doc
}

#[test]
fn a_collection_with_a_graphic_and_an_asset_round_trips_through_a_zip() {
    let dir = Dir::new("zip");
    let doc = sample(&dir.0);
    let options = Options { root: Some(dir.0.clone()), ..Options::default() };

    let (bundle, bytes) = export_zip(&doc, &options).expect("exporting");
    assert_eq!(bundle.assets.len(), 1, "the asset did not go in: {:?}", bundle.skipped);
    assert!(bundle.skipped.is_empty(), "{:?}", bundle.skipped);

    let at = dir.join("show.zip");
    std::fs::write(&at, &bytes).unwrap();
    let back_to = Dir::new("zip-back");
    let imported = import(&at, Some(&back_to.0)).expect("importing");

    assert!(imported.relink.is_empty(), "{:?}", imported.relink);
    assert_eq!(imported.document, doc, "the document came back different");
    assert_eq!(std::fs::read(back_to.join("media/logo.png")).unwrap(), LOGO);
}

#[test]
fn a_directory_export_reads_back_the_same_as_the_zip() {
    let dir = Dir::new("dir");
    let doc = sample(&dir.0);
    let options = Options { root: Some(dir.0.clone()), ..Options::default() };

    let out = Dir::new("dir-out");
    export_dir(&doc, &out.0, &options).expect("exporting");
    assert!(out.join("collection.json").is_file());
    assert!(out.join("bundle.json").is_file());
    assert!(out.join("assets/media/logo.png").is_file());

    let back_to = Dir::new("dir-back");
    let imported = import(&out.0, Some(&back_to.0)).expect("importing");
    assert!(imported.relink.is_empty(), "{:?}", imported.relink);
    assert_eq!(imported.document, doc);
}

#[test]
fn the_bundle_names_every_plugin_the_collection_needs() {
    let dir = Dir::new("requires");
    let doc = sample(&dir.0);
    let versions: BTreeMap<String, String> =
        [("ograf".to_string(), "0.2.0".to_string())].into_iter().collect();
    let options = Options { root: Some(dir.0.clone()), versions };

    let (bundle, _) = export_zip(&doc, &options).unwrap();
    let ograf = bundle.requires.iter().find(|r| r.plugin == "ograf").expect("the graphic's plugin");
    assert_eq!(ograf.versions, "^0.2.0");
    assert_eq!(ograf.provides, vec!["ograf/lower-third"]);
    let chroma = bundle.requires.iter().find(|r| r.plugin == "chroma").expect("the filter's plugin");
    assert_eq!(chroma.versions, "*", "a plugin we have no version for asks for any");
}

#[test]
fn a_missing_asset_names_the_file_and_the_item_that_draws_it() {
    let dir = Dir::new("missing");
    let doc = sample(&dir.0);
    let options = Options { root: Some(dir.0.clone()), ..Options::default() };
    let (_, bytes) = export_zip(&doc, &options).unwrap();

    // The bundle is rebuilt without the asset, which is what a hand edited
    // share, or one trimmed to fit an email, actually looks like.
    let mut members = crate::zip::read(&bytes).unwrap();
    members.remove("assets/media/logo.png");
    let mut trimmed = Zip::new();
    for (name, data) in &members {
        trimmed.add(name, data);
    }
    let at = dir.join("trimmed.zip");
    std::fs::write(&at, trimmed.finish()).unwrap();

    let imported = import(&at, None).expect("a bundle missing a file still imports");
    assert_eq!(imported.relink.len(), 1, "{:?}", imported.relink);
    let one = &imported.relink[0];
    assert_eq!(one.path, "media/logo.png");
    assert!(one.reason.contains("does not carry"), "{}", one.reason);
    assert_eq!(
        one.items,
        vec!["Wide with lower third: lower third"],
        "the report has to say what will be blank"
    );
    // The scenes still came across: a partial import succeeds visibly.
    assert_eq!(imported.document.scenes.len(), 1);
}

#[test]
fn an_asset_whose_bytes_changed_is_reported_rather_than_used() {
    let dir = Dir::new("swapped");
    let doc = sample(&dir.0);
    let options = Options { root: Some(dir.0.clone()), ..Options::default() };
    let (_, bytes) = export_zip(&doc, &options).unwrap();

    let mut members = crate::zip::read(&bytes).unwrap();
    members.insert("assets/media/logo.png".into(), b"a different picture entirely".to_vec());
    let mut swapped = Zip::new();
    for (name, data) in &members {
        swapped.add(name, data);
    }
    let at = dir.join("swapped.zip");
    std::fs::write(&at, swapped.finish()).unwrap();

    let imported = import(&at, None).unwrap();
    assert_eq!(imported.relink.len(), 1);
    assert!(imported.relink[0].reason.contains("not the file"), "{:?}", imported.relink[0]);
}

#[test]
fn an_absolute_asset_path_is_refused_at_export_with_what_to_do() {
    let dir = Dir::new("absolute");
    let mut doc = sample(&dir.0);
    let id = *doc.assets.keys().next().unwrap();
    doc.assets.get_mut(&id).unwrap().path = dir.join("media/logo.png").to_string_lossy().into();
    let options = Options { root: Some(dir.0.clone()), ..Options::default() };

    let (bundle, _) = export_zip(&doc, &options).unwrap();
    assert!(bundle.assets.is_empty());
    assert_eq!(bundle.skipped.len(), 1);
    assert!(bundle.skipped[0].contains("relative"), "{}", bundle.skipped[0]);
}

#[test]
fn a_bare_document_with_no_envelope_still_imports() {
    let dir = Dir::new("bare");
    let mut doc = Collection::new("Just scenes", Canvas::default());
    doc.scenes.push(Scene::new("wide"));
    std::fs::write(dir.join("collection.json"), doc.to_json()).unwrap();

    let imported = import(&dir.0, None).expect("a folder with just a document");
    assert_eq!(imported.document.scenes.len(), 1);
    assert_eq!(imported.bundle.written_by, "unknown");
}

#[test]
fn a_file_that_is_not_a_bundle_says_what_to_pass_instead() {
    let dir = Dir::new("notabundle");
    std::fs::write(dir.join("notes.txt"), b"shopping list").unwrap();
    let err = import(&dir.join("notes.txt"), None).unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("scene.export"), "{text}");
}

#[test]
fn a_bundle_from_a_newer_build_says_to_upgrade_rather_than_half_reading_it() {
    let dir = Dir::new("newer");
    let doc = Collection::new("future", Canvas::default());
    std::fs::write(dir.join("collection.json"), doc.to_json()).unwrap();
    let mut bundle: Value = serde_json::to_value(Bundle {
        bundle_version: BUNDLE_VERSION + 1,
        id: doc.id,
        name: doc.name.clone(),
        canvas: doc.canvas,
        written_by: "godwinmix 9.9.9".into(),
        requires: Vec::new(),
        assets: Vec::new(),
        skipped: Vec::new(),
    })
    .unwrap();
    bundle["something_new"] = json!(true);
    std::fs::write(dir.join("bundle.json"), bundle.to_string()).unwrap();

    let err = import(&dir.0, None).unwrap_err();
    assert!(format!("{err:#}").contains("Upgrade GodwinMix"), "{err:#}");
}
