//! The gallery on a real disk, drawn with real elements.

use super::preview::{preview, Backdrop};
use super::*;
use godwinmix_protocol::gallery::{GalleryKind, Origin, Zone};

const STRAP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:gmx="https://godwinmix.dev/ns/template" width="1920" height="1080" viewBox="0 0 1920 1080">
<metadata><gmx:template title="Test strap" description="A bar"/><gmx:field name="headline" label="Headline" default="Hello"/></metadata>
<rect x="96" y="860" width="1200" height="120" fill="#c8102e"/>
<text x="130" y="940" font-size="56" fill="#fff" data-fit-width="1100">{{headline}}</text></svg>"##;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-gallery-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn drawing() -> bool {
    let _ = gstreamer::init();
    let ok = crate::probe::exists("rsvgdec") && crate::probe::exists("jpegenc");
    if !ok {
        println!("skipping: this GStreamer cannot draw an SVG");
    }
    ok
}

#[test]
fn a_name_becomes_a_slug_and_only_a_slug_is_an_id() {
    assert_eq!(slug("Storm warning, lower third!"), "storm-warning-lower-third");
    assert_eq!(slug("  ##  "), "graphic");
    assert_eq!(slug(&"x".repeat(100)).len(), MAX_ID);
    assert!(is_slug("news-24") && !is_slug("News") && !is_slug("a/b") && !is_slug("-a"));
}

#[test]
fn a_template_is_saved_listed_with_its_fields_and_drawn() {
    if !drawing() {
        return;
    }
    let g = scratch("template");
    let mut d = draft::svg(STRAP).expect("the strap reads");
    assert_eq!(d.manifest.kind, "template");
    d.manifest.name = "Test strap".into();
    d.manifest.values.insert("headline".into(), "Storm warning".into());
    store::write(&g, "test-strap", &d, false).expect("written");
    assert!(g.join(MARKER).is_file(), "the folder is marked so the media library skips it");
    let refused = store::write(&g, "test-strap", &d, false).unwrap_err().to_string();
    assert!(refused.contains("replace: true"), "{refused}");

    let e = store::find(&g, "test-strap").unwrap();
    assert_eq!(e.item.kind, GalleryKind::Template);
    assert_eq!(e.item.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["headline"]);
    assert_eq!(e.item.values["headline"], "Storm warning");
    assert!(e.item.uri.as_deref().unwrap().starts_with("template:"));
    assert!(e.item.transparent && !e.item.moves);

    let still = preview(&g, &e, 480, Backdrop::Checker, &Default::default()).expect("drawn");
    assert_eq!((still.width, still.height, still.from), (480, 270, "drawn"));
    assert!(still.jpeg.starts_with(&[0xff, 0xd8]), "a JPEG");
    let again = preview(&g, &e, 480, Backdrop::Checker, &Default::default()).unwrap();
    assert_eq!(again.jpeg, still.jpeg, "the second ask is the cached picture");
    let bad = serde_json::json!({"title": "x"}).as_object().unwrap().clone();
    let err = preview(&g, &e, 480, Backdrop::Checker, &bad).unwrap_err().to_string();
    assert!(err.contains("headline"), "a field it does not have names the ones it has: {err}");
    let _ = std::fs::remove_dir_all(&g);
}

#[test]
fn the_pack_and_the_starters_are_listed_read_only() {
    let g = scratch("shipped");
    let (all, errors) = store::list(&g);
    assert!(errors.is_empty(), "{errors:?}");
    let strap = all.iter().find(|e| e.item.id == "news-lower-third").expect("the pack is listed");
    assert_eq!(strap.item.origin, Origin::Shipped);
    let bg = all.iter().find(|e| e.item.id == "blue-gradient").expect("the starter is listed");
    assert_eq!((bg.item.kind, bg.item.zone, bg.item.transparent), (GalleryKind::Image, Zone::Full, false));
    assert!(g.join(".shipped/blue-gradient/background.svg").is_file(), "written out to be drawn");
    assert!(store::remove(&g, "news-lower-third").is_err());
    assert!(edit::edit(&g, "blue-gradient", |m| m.name = "x".into()).unwrap_err().to_string().contains("Duplicate"));

    let copy = edit::duplicate(&g, "news-lower-third", Some("Our strap")).expect("a pack template copies");
    assert_eq!((copy.item.id.as_str(), copy.item.origin, copy.item.kind), ("our-strap", Origin::Uploaded, GalleryKind::Template));
    let edited = edit::edit(&g, "our-strap", |m| m.tags = vec!["ours".into()]).unwrap();
    assert_eq!(edited.item.tags, ["ours"]);
    store::remove(&g, "our-strap").expect("a copy can be deleted");
    assert!(store::find(&g, "our-strap").is_err());
    let _ = std::fs::remove_dir_all(&g);
}

#[test]
fn every_kind_of_preview_draws_something() {
    if !drawing() {
        return;
    }
    let g = scratch("kinds");
    let (all, _) = store::list(&g);
    for id in ["blue-gradient", "score-bug", "title-card"] {
        let e = all.iter().find(|e| e.item.id == id).unwrap();
        let s = preview(&g, e, 320, Backdrop::Colour([0, 0, 0]), &Default::default()).unwrap_or_else(|err| panic!("{id}: {err:#}"));
        assert_eq!(s.from, "drawn", "{id}");
    }
    let mut ticker = draft::source("ticker:", toml::toml! { items = ["Rain at six", "Roads closed"] }).unwrap();
    ticker.manifest.name = "Crawl".into();
    store::write(&g, "crawl", &ticker, false).unwrap();
    let e = store::find(&g, "crawl").unwrap();
    assert_eq!((e.item.kind, e.item.zone, e.item.moves), (GalleryKind::Ticker, Zone::Bottom, true));
    preview(&g, &e, 320, Backdrop::Checker, &Default::default()).expect("a ticker draws");

    let mut page = draft::html("<!doctype html><html><body style=\"background:transparent\">Hi</body></html>", Vec::new()).unwrap();
    page.manifest.name = "Page".into();
    store::write(&g, "page", &page, false).unwrap();
    let s = preview(&g, &store::find(&g, "page").unwrap(), 320, Backdrop::Checker, &Default::default()).expect("a page has a card");
    assert_eq!(s.from, "card");
    let _ = std::fs::remove_dir_all(&g);
}

#[test]
fn an_export_imports_again_and_a_bad_file_is_refused_with_a_fix() {
    let g = scratch("bundle");
    let mut d = draft::svg(STRAP).unwrap();
    d.manifest.name = "Strap".into();
    store::write(&g, "strap", &d, false).unwrap();
    let zip = g.join("exports/look.zip");
    let entries = vec![store::find(&g, "strap").unwrap(), store::find(&g, "blue-gradient").unwrap()];
    assert!(bundle::export(&entries, &zip).unwrap() > 0);
    let back = bundle::read_path(&zip);
    assert_eq!(back.len(), 2, "{:?}", back.iter().map(|(n, _)| n).collect::<Vec<_>>());
    for (name, r) in &back {
        let d = r.as_ref().unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!d.manifest.name.is_empty());
    }

    let refused = bundle::read_bytes("notes.txt", b"just some words".to_vec());
    let err = refused[0].1.as_ref().unwrap_err();
    assert!(err.reason.contains("notes.txt") && err.fix.contains("SVG"), "{err:?}");
    let empty_svg = draft::svg("<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>").unwrap_err();
    assert!(empty_svg.fix.contains("viewBox"), "{empty_svg:?}");
    let _ = std::fs::remove_dir_all(&g);
}

#[test]
fn zones_place_a_small_picture_in_its_corner_and_a_frame_over_the_canvas() {
    let t = place::transform(Zone::Bug, (1920, 1080), Some((300, 300)));
    assert_eq!(t["position"]["x"], 1824.0 - 300.0);
    assert_eq!(t["position"]["y"], 54.0);
    let whole = place::transform(Zone::LowerThird, (1280, 720), Some((1920, 1080)));
    assert_eq!(whole["frame"]["w"], 1280);
    assert_eq!(detect::zone(GalleryKind::Image, true, Some((1400, 200))), Zone::LowerThird);
    assert_eq!(detect::zone(GalleryKind::Image, false, Some((1920, 1080))), Zone::Full);
}
