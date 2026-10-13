//! Every spelling a reference to a moved file can have.

use super::*;

fn moved(root: &Path, from: &str, to: &str) -> Moved {
    Moved { from: from.into(), to: to.into(), from_path: root.join(from), to_path: root.join(to), also: Vec::new() }
}

#[test]
fn every_spelling_of_a_moved_file_follows_and_a_folder_name_alone_does_not() {
    let root = std::env::temp_dir().join(format!("gmx-refs-map-{}", std::process::id()));
    std::fs::create_dir_all(root.join("Talks")).unwrap();
    let map = Mapping::new(&root, &[moved(&root, "logo.png", "Brand/logo.png"), moved(&root, "Sermons", "Talks")]);
    assert_eq!(map.value("logo.png").as_deref(), Some("Brand/logo.png"));
    assert_eq!(map.value("media:Sermons/a.png").as_deref(), Some("media:Talks/a.png"));
    assert_eq!(map.address("template:Sermons/card.svg").as_deref(), Some("template:Talks/card.svg"));
    assert_eq!(map.value("Sermons"), None, "a bare folder name could be a scene's");
    let abs = root.join("Sermons").join("a.mp4").display().to_string();
    assert_eq!(map.address(&abs), Some(root.join("Talks").join("a.mp4").display().to_string()));
    let uri = godwinmix_core::input::file_uri(&root.join("Sermons").join("my clip.mp4"));
    let next = map.address(&uri).unwrap();
    assert!(next.starts_with("file:") && next.contains("Talks/my%20clip.mp4"), "{next}");
    assert_eq!(map.address("rtmp://host/Sermons/a.mp4"), None);
    let svg = r#"<image href="logo.png"/><image xlink:href='Sermons/x.png'/><text>logo.png</text>"#;
    assert_eq!(map.text(svg).unwrap(), r#"<image href="Brand/logo.png"/><image xlink:href='Talks/x.png'/><text>logo.png</text>"#);
    assert_eq!(map.text("nothing here"), None);
    let _ = std::fs::remove_dir_all(&root);
}
