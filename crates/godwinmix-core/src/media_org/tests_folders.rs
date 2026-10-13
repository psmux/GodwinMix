//! Folders on disk: made, listed, nested, moved, renamed and removed, and
//! files moved between them.

use super::tests::scratch;
use super::*;

#[test]
fn folders_are_real_directories_any_depth_and_one_made_by_hand_shows_up() {
    let dir = scratch("tree");
    assert_eq!(make_folder(&dir, "Sermons/2026/Easter").unwrap(), ("Sermons/2026/Easter".into(), true));
    assert!(dir.join("Sermons/2026/Easter").is_dir());
    assert_eq!(make_folder(&dir, "sermons").unwrap(), ("Sermons".into(), false), "the one there, in its own case");
    std::fs::create_dir(dir.join("Ads")).unwrap();
    std::fs::create_dir(dir.join(".hidden")).unwrap();
    assert_eq!(list_folders(&dir, 8), ["Ads", "Sermons", "Sermons/2026", "Sermons/2026/Easter"]);
    assert_eq!(make_folder(&dir, "a.mp4").unwrap_err().field, "name", "a file is in the way");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_moves_and_renames_on_disk_and_never_into_itself() {
    let dir = scratch("move");
    make_folder(&dir, "Sermons/2026").unwrap();
    make_folder(&dir, "Archive").unwrap();
    let m = move_folder(&dir, "Sermons", Some("Archive"), None).unwrap();
    assert_eq!((m.from.as_str(), m.to.as_str()), ("Sermons", "Archive/Sermons"));
    assert!(dir.join("Archive/Sermons/c.png").is_file(), "its files went with it");
    let r = move_folder(&dir, "Archive", Some("Archive/Sermons/2026"), None).unwrap_err();
    assert_eq!(r.field, "into");
    assert!(r.message.contains("inside itself") && r.message.contains("Pick a folder outside"), "{}", r.message);
    assert_eq!(move_folder(&dir, "Archive", Some("Archive"), None).unwrap_err().field, "into");
    let m = move_folder(&dir, "Archive/Sermons", Some(""), Some("Talks")).unwrap();
    assert_eq!(m.to, "Talks", "to the top with a new name in one go");
    make_folder(&dir, "Ads").unwrap();
    assert_eq!(move_folder(&dir, "Talks", None, Some("ads")).unwrap_err().field, "to", "a name taken but for case");
    assert_eq!(move_folder(&dir, "Talks", None, Some("talks")).unwrap().to, "talks", "a change of case is a rename");
    assert!(move_folder(&dir, "Nope", Some(""), None).unwrap_err().missing);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn removing_a_folder_moves_what_is_in_it_up_or_to_the_top_and_deletes_no_file() {
    let dir = scratch("remove");
    make_folder(&dir, "Sermons/2026/Easter").unwrap();
    std::fs::write(dir.join("Sermons/2026/d.mp4"), b"x").unwrap();
    std::fs::write(dir.join("Sermons/2026/Thumbs.db"), b"x").unwrap();
    let moved = remove_folder(&dir, "Sermons/2026", Contents::Up).unwrap();
    let to: Vec<&str> = moved.iter().map(|m| m.to.as_str()).collect();
    assert_eq!(to.len(), 2);
    assert!(to.contains(&"Sermons/d.mp4") && to.contains(&"Sermons/Easter"), "{to:?}");
    assert!(!dir.join("Sermons/2026").exists() && dir.join("Sermons/d.mp4").is_file());
    let moved = remove_folder(&dir, "Sermons", Contents::Top).unwrap();
    assert_eq!(moved.len(), 3);
    assert!(dir.join("c.png").is_file() && dir.join("d.mp4").is_file() && dir.join("Easter").is_dir());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn removing_a_folder_whose_contents_would_land_on_a_taken_name_moves_nothing() {
    let dir = scratch("blocked");
    std::fs::write(dir.join("Sermons/A.MP4"), b"x").unwrap();
    let r = remove_folder(&dir, "Sermons", Contents::Up).unwrap_err();
    assert_eq!(r.valid, ["A.MP4"]);
    assert!(r.message.contains("Rename or move those first"), "{}", r.message);
    assert!(dir.join("Sermons/A.MP4").is_file() && dir.join("Sermons/c.png").is_file(), "nothing moved");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_moves_with_its_converted_copy_and_a_rename_keeps_an_extension() {
    let dir = scratch("file");
    std::fs::write(dir.join("a.web.mp4"), b"x").unwrap();
    let m = plan_file(&dir, "a.mp4", Some("Sermons"), Some("Easter.mp4")).unwrap();
    move_file(&m).unwrap();
    assert_eq!(m.to, "Sermons/Easter.mp4");
    assert!(dir.join("Sermons/Easter.mp4").is_file() && dir.join("Sermons/Easter.web.mp4").is_file());
    assert!(!dir.join("a.mp4").exists() && !dir.join("a.web.mp4").exists());
    assert_eq!(plan_file(&dir, "b.mov", None, Some("b.txt")).unwrap_err().field, "new_name");
    assert_eq!(plan_file(&dir, "b.mov", Some("Sermons"), Some("easter.MP4")).unwrap_err().field, "new_name");
    assert!(plan_file(&dir, "b.mov", Some("Nope"), None).unwrap_err().missing);
    assert!(plan_file(&dir, "../b.mov", None, None).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
