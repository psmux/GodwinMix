//! The HTML pack is what a model copies, so every design in it has to pass
//! the check with nothing to say, not only without errors.

use super::check::check;
use super::pack::{self, PACK};
use crate::graphics::template::FieldType;

#[test]
fn every_design_in_the_pack_passes_the_check_with_nothing_to_say() {
    for (name, html) in PACK {
        let (problems, meta) = check(html);
        assert!(problems.is_empty(), "{name}: {problems:#?}");
        let meta = meta.unwrap();
        assert!(meta.title.is_some() && !meta.description.is_empty() && meta.category.is_some(), "{name} says what it is");
        for f in &meta.fields {
            assert!(f.kind == FieldType::Image || !f.default.is_empty() || f.name == "until", "{name} field {} has a default to show", f.name);
        }
        assert!(meta.fields.iter().any(|f| f.name == "accent" && f.kind == FieldType::Color), "{name} takes the station's accent colour");
        if !meta.opaque {
            assert!(meta.out_ms.is_some(), "{name} overlays the picture, so it says how long its way out takes");
        }
    }
}

#[test]
fn the_pack_reads_and_each_page_is_written_where_the_renderer_can_load_it() {
    let read = pack::pack();
    assert_eq!(read.len(), PACK.len(), "every pack file reads as a template");
    let t = pack::load("lower-third-glass").unwrap();
    let file = t.file.clone().unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), t.html);
    assert!(t.info.uri == "html:lower-third-glass" && t.info.out_ms == Some(600));
    let e = pack::load("no-such-design").unwrap_err().to_string();
    assert!(e.contains("lower-third-glass") && e.contains("template.list"), "{e}");
}

#[test]
fn a_saved_template_is_checked_first_and_listed_after() {
    let dir = std::env::temp_dir().join(format!("gmx-html-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (_, glass) = PACK.iter().find(|(n, _)| *n == "lower-third-glass").unwrap();
    let saved = pack::save(&dir, "ours", glass, false).unwrap();
    assert_eq!(saved.info.name, "ours.html");
    assert!(pack::save(&dir, "ours", glass, false).unwrap_err().to_string().contains("replace"));
    let broken = glass.replace("background: transparent", "background: #000");
    let e = pack::save(&dir, "broken", &broken, false).unwrap_err().to_string();
    assert!(e.contains("background: transparent") && !dir.join("broken.html").exists(), "{e}");
    let (listed, errors) = pack::library(&dir);
    assert_eq!((listed.len(), errors.len()), (1, 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_design_is_a_gallery_starter_drawn_as_an_html_template() {
    use godwinmix_protocol::gallery::{GalleryKind, Origin};
    let g = std::env::temp_dir().join(format!("gmx-html-gallery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&g);
    let (all, errors) = crate::gallery::store::list(&g);
    assert!(errors.is_empty(), "{errors:?}");
    for (name, _) in PACK.iter().filter(|(n, _)| !n.starts_with("set-")) {
        let e = all.iter().find(|e| e.item.id == *name).unwrap_or_else(|| panic!("{name} is in the gallery"));
        assert_eq!((e.item.kind, e.item.origin), (GalleryKind::Html, Origin::Shipped), "{name}");
        assert!(e.item.uri.as_deref().is_some_and(|u| u.starts_with("html:")), "{name} is drawn as a template: {:?}", e.item.uri);
        assert!(e.item.fields.iter().any(|f| f.name == "accent"), "{name} lists its fields");
        assert!(e.preview_file().is_some(), "{name} has a picture for its card");
    }
    for set in ["studio-newsroom", "studio-ring"] {
        let e = all.iter().find(|e| e.item.id == set).unwrap_or_else(|| panic!("{set} is in the gallery"));
        assert_eq!(e.item.kind, GalleryKind::Set);
        let spec = e.manifest.set.as_ref().expect("a set says what it is made of");
        let dir = e.dir().expect("written out");
        assert!(dir.join(&spec.background).is_file() && dir.join(spec.foreground.as_ref().unwrap()).is_file());
    }
    let _ = std::fs::remove_dir_all(&g);
}
