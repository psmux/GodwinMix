//! The tags file, the tag rules, the path rules and the filter.

use super::*;
use std::path::PathBuf;

pub fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-org-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Sermons")).unwrap();
    for f in ["a.mp4", "b.mov", "Sermons/c.png"] {
        std::fs::write(dir.join(f), b"x").unwrap();
    }
    dir
}

pub fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_missing_tags_file_reads_as_empty_and_the_first_change_writes_it() {
    let dir = scratch("missing");
    assert_eq!(load(&dir), Tags::default());
    tag_files(&dir, &names(&["a.mp4"]), None, &names(&["easter"]), &[]).unwrap();
    assert!(dir.join(FILE).is_file());
    assert!(!dir.join(format!("{FILE}.part")).exists(), "the temporary file was renamed into place");
    assert_eq!(load(&dir).of("a.mp4"), ["easter"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_damaged_tags_file_reads_as_empty_and_is_set_aside_not_written_over() {
    let dir = scratch("damaged");
    std::fs::write(dir.join(FILE), b"{ not json").unwrap();
    assert_eq!(load(&dir), Tags::default(), "never fatal");
    assert_eq!(std::fs::read(dir.join(FILE)).unwrap(), b"{ not json", "a read leaves it alone");
    tag_files(&dir, &names(&["b.mov"]), None, &names(&["promo"]), &[]).unwrap();
    assert_eq!(std::fs::read(dir.join(DAMAGED)).unwrap(), b"{ not json", "kept for a person to look at");
    assert_eq!(load(&dir).of("b.mov"), ["promo"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_gone_from_disk_loses_its_tags_and_a_move_carries_them() {
    let dir = scratch("vanished");
    tag_files(&dir, &names(&["a.mp4", "b.mov", "Sermons/c.png"]), None, &names(&["x"]), &[]).unwrap();
    std::fs::remove_file(dir.join("a.mp4")).unwrap();
    let tags = load(&dir);
    assert!(!tags.files.contains_key("a.mp4"));
    assert!(!std::fs::read_to_string(dir.join(FILE)).unwrap().contains("a.mp4"), "written back tidy");
    std::fs::write(dir.join("new.mp4"), b"x").unwrap();
    assert!(load(&dir).of("new.mp4").is_empty(), "a file added by hand has no tags");
    std::fs::rename(dir.join("Sermons"), dir.join("Talks")).unwrap();
    moved(&dir, "Sermons", "Talks");
    assert_eq!(load(&dir).of("Talks/c.png"), ["x"], "a folder's move carries its files' tags");
    forget(&dir, "b.mov");
    assert!(load(&dir).of("b.mov").is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tags_are_trimmed_and_case_insensitive_unique_in_the_case_first_typed() {
    let dir = scratch("case");
    let out = tag_files(&dir, &names(&["a.mp4"]), Some(&names(&["  Easter ", "choir", "easter", "", "Choir"])), &[], &[]).unwrap();
    assert_eq!(out[0], ["Easter", "choir"]);
    let out = tag_files(&dir, &names(&["b.mov"]), None, &names(&["EASTER", "youth"]), &[]).unwrap();
    assert_eq!(out[0], ["Easter", "youth"], "spelled the way the library has it");
    let out = tag_files(&dir, &names(&["a.mp4"]), None, &[], &names(&["easter"])).unwrap();
    assert_eq!(out[0], ["choir"]);
    let long = tag_files(&dir, &names(&["a.mp4"]), None, &["x".repeat(41)], &[]).unwrap_err();
    assert_eq!(long.field, "tags");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_path_stays_inside_the_library_with_names_every_platform_can_hold() {
    assert_eq!(rel_path("Sermons/2026/", "f").unwrap(), "Sermons/2026");
    assert_eq!(rel_path("", "f").unwrap(), "");
    for bad in ["../x", "a/../../x", "/etc", "C:/Windows", "a\\b", "a//b", ".hidden", "CON", "aux.mp4", "lpt1", "a?", "a*b", "dot.", "a\u{1}b"] {
        let r = rel_path(bad, "folder").unwrap_err();
        assert_eq!(r.field, "folder", "{bad:?}");
        assert!(r.message.contains("Pick") || r.message.contains("Give"), "names the next step: {}", r.message);
    }
    assert!(segment("Com10", "f").is_ok(), "only COM1 to COM9 are kept");
    let dir = scratch("inside");
    assert!(inside(&dir, "Sermons/new.mp4", "name").is_ok(), "the last name need not exist yet");
    assert!(inside(&dir, "Nope/new.mp4", "name").unwrap_err().missing);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_filter_matches_a_folder_its_subfolders_tags_and_words() {
    let item = |name: &str, tags: &[&str]| {
        let mut i: crate::media::MediaItem = serde_json::from_value(serde_json::json!({
            "name": name, "path": name, "size_bytes": 1, "duration_ms": null, "has_video": true, "has_audio": true
        }))
        .unwrap();
        i.tags = names(tags);
        i
    };
    let all = vec![item("Sermons/sunday.mp4", &["Easter"]), item("Sermons/2026/promo.mov", &["easter", "choir"]), item("bg.png", &[])];
    let run = |f: Filter| {
        let mut v = all.clone();
        f.apply(&mut v);
        v.into_iter().map(|i| i.name).collect::<Vec<_>>()
    };
    assert_eq!(run(Filter::default()).len(), 3);
    assert_eq!(run(Filter { folder: Some("sermons".into()), ..Default::default() }), ["Sermons/sunday.mp4"]);
    assert_eq!(run(Filter { folder: Some("Sermons".into()), subfolders: true, ..Default::default() }).len(), 2);
    assert_eq!(run(Filter { folder: Some("".into()), ..Default::default() }), ["bg.png"]);
    assert_eq!(run(Filter { tags: names(&["EASTER", "choir"]), ..Default::default() }), ["Sermons/2026/promo.mov"]);
    assert_eq!(run(Filter { query: Some("2026 choir".into()), ..Default::default() }), ["Sermons/2026/promo.mov"]);
}
