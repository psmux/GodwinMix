use super::*;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-labels-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["a.mp4", "b.mov", "c.png"] {
        std::fs::write(dir.join(f), b"x").unwrap();
    }
    dir
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn into(folder: &str) -> Change {
    Change { folder: Some(folder.into()), ..Default::default() }
}

#[test]
fn a_missing_sidecar_reads_as_empty_and_the_first_change_writes_it() {
    let dir = scratch("missing");
    assert_eq!(load(&dir), Labels::default());
    update(&dir, |l| set(l, &names(&["a.mp4"]), &into("Sermons"))).unwrap();
    assert!(dir.join(FILE).is_file());
    assert_eq!(load(&dir).of("a.mp4").folder.as_deref(), Some("Sermons"));
    assert!(!dir.join(format!("{FILE}.part")).exists(), "the temporary file was renamed into place");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_damaged_sidecar_reads_as_empty_and_is_set_aside_not_written_over() {
    let dir = scratch("damaged");
    std::fs::write(dir.join(FILE), b"{ not json").unwrap();
    assert_eq!(load(&dir), Labels::default(), "never fatal");
    assert_eq!(std::fs::read(dir.join(FILE)).unwrap(), b"{ not json", "a read leaves it alone");
    update(&dir, |l| set(l, &names(&["b.mov"]), &into("Ads"))).unwrap();
    assert_eq!(std::fs::read(dir.join(DAMAGED)).unwrap(), b"{ not json", "kept for a person to look at");
    assert_eq!(load(&dir).of("b.mov").folder.as_deref(), Some("Ads"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_gone_from_disk_is_dropped_on_the_next_read_and_one_added_by_hand_is_in_all() {
    let dir = scratch("vanished");
    let tag = Change { add: names(&["easter"]), ..Default::default() };
    update(&dir, |l| set(l, &names(&["a.mp4", "b.mov"]), &tag)).unwrap();
    std::fs::remove_file(dir.join("a.mp4")).unwrap();
    let labels = load(&dir);
    assert!(!labels.files.contains_key("a.mp4"));
    assert!(labels.files.contains_key("b.mov"));
    let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
    assert!(!text.contains("a.mp4"), "written back tidy: {text}");
    std::fs::write(dir.join("new.mp4"), b"x").unwrap();
    assert!(load(&dir).of("new.mp4").is_empty(), "no folder and no tags: All only");
    forget(&dir, "b.mov");
    assert!(load(&dir).files.is_empty(), "a removed file takes its labels with it");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tags_are_trimmed_and_case_insensitive_unique_in_the_case_first_typed() {
    let mut l = Labels::default();
    let first = Change { tags: Some(names(&["  Easter ", "choir", "easter", "", "Choir"])), ..Default::default() };
    let (out, _) = set(&mut l, &names(&["a.mp4"]), &first).unwrap();
    assert_eq!(out[0].tags, ["Easter", "choir"]);
    let other = Change { add: names(&["EASTER", "youth"]), ..Default::default() };
    let (out, _) = set(&mut l, &names(&["b.mov"]), &other).unwrap();
    assert_eq!(out[0].tags, ["Easter", "youth"], "spelled the way the library has it");
    let off = Change { remove: names(&["easter"]), ..Default::default() };
    let (out, _) = set(&mut l, &names(&["a.mp4"]), &off).unwrap();
    assert_eq!(out[0].tags, ["choir"]);
    let long = Change { add: vec!["x".repeat(41)], ..Default::default() };
    assert_eq!(set(&mut l, &names(&["a.mp4"]), &long).unwrap_err().field, "tags");
}

#[test]
fn naming_only_the_tags_leaves_the_folder_and_the_other_way_round() {
    let mut l = Labels::default();
    set(&mut l, &names(&["a.mp4"]), &Change { folder: Some("Ads".into()), add: names(&["promo"]), ..Default::default() }).unwrap();
    let (out, _) = set(&mut l, &names(&["a.mp4"]), &Change { add: names(&["new"]), ..Default::default() }).unwrap();
    assert_eq!(out[0].folder.as_deref(), Some("Ads"));
    let (out, _) = set(&mut l, &names(&["a.mp4"]), &into("All")).unwrap();
    assert_eq!((out[0].folder.clone(), out[0].tags.clone()), (None, names(&["promo", "new"])));
}

#[test]
fn folders_are_one_level_plain_names_matched_without_case() {
    let mut l = Labels::default();
    assert_eq!(add_folder(&mut l, "  Sermons  ").unwrap(), ("Sermons".into(), true));
    assert_eq!(add_folder(&mut l, "sermons").unwrap(), ("Sermons".into(), false));
    for bad in ["", "a/b", "a\\b", "All", "all", &"x".repeat(61)] {
        assert_eq!(add_folder(&mut l, bad).unwrap_err().field, "folder", "{bad:?}");
    }
    let (_, made) = set(&mut l, &names(&["a.mp4"]), &into("Backgrounds")).unwrap();
    assert_eq!(made.as_deref(), Some("Backgrounds"), "a folder named by a move is made");
    assert_eq!(l.folders, ["Sermons", "Backgrounds"]);
}

#[test]
fn renaming_a_folder_takes_its_files_and_removing_one_leaves_them_in_all() {
    let mut l = Labels::default();
    set(&mut l, &names(&["a.mp4", "b.mov"]), &into("Ad")).unwrap();
    set(&mut l, &names(&["c.png"]), &into("Sermons")).unwrap();
    assert_eq!(rename_folder(&mut l, "ad", "Ads").unwrap(), names(&["a.mp4", "b.mov"]));
    assert_eq!(l.of("a.mp4").folder.as_deref(), Some("Ads"));
    let clash = rename_folder(&mut l, "Ads", "sermons").unwrap_err();
    assert_eq!(clash.field, "to");
    assert!(clash.message.contains("Pick another name"), "{}", clash.message);
    let missing = rename_folder(&mut l, "Nope", "X").unwrap_err();
    assert_eq!(missing.valid, Some(names(&["Ads", "Sermons"])));
    assert!(missing.message.contains("media.folder.add"), "names the next step: {}", missing.message);
    assert_eq!(remove_folder(&mut l, "ADS").unwrap(), names(&["a.mp4", "b.mov"]));
    assert_eq!(l.folders, ["Sermons"]);
    assert!(l.of("a.mp4").folder.is_none(), "still in the library, in All");
}

#[test]
fn counts_list_empty_folders_and_the_most_used_tag_first() {
    let mut l = Labels::default();
    add_folder(&mut l, "Empty").unwrap();
    set(&mut l, &names(&["a.mp4", "b.mov"]), &Change { folder: Some("Ads".into()), add: names(&["promo"]), ..Default::default() }).unwrap();
    set(&mut l, &names(&["c.png"]), &Change { add: names(&["choir"]), ..Default::default() }).unwrap();
    let c = counts(&l);
    let f: Vec<(String, usize)> = c.folders.into_iter().map(|m| (m.name, m.count)).collect();
    assert_eq!(f, [("Empty".into(), 0), ("Ads".into(), 2)]);
    let t: Vec<(String, usize)> = c.tags.into_iter().map(|m| (m.name, m.count)).collect();
    assert_eq!(t, [("promo".into(), 2), ("choir".into(), 1)]);
}

#[test]
fn a_filter_matches_folder_tags_and_words_in_the_name_or_the_tags() {
    let item = |name: &str, folder: Option<&str>, tags: &[&str]| {
        let mut i: crate::media::MediaItem = serde_json::from_value(serde_json::json!({
            "name": name, "path": name, "size_bytes": 1, "duration_ms": null, "has_video": true, "has_audio": true
        }))
        .unwrap();
        i.folder = folder.map(str::to_string);
        i.tags = names(tags);
        i
    };
    let all = vec![item("sunday.mp4", Some("Sermons"), &["Easter"]), item("promo.mov", Some("Ads"), &["easter", "choir"]), item("bg.png", None, &[])];
    let run = |f: Filter| {
        let mut v = all.clone();
        f.apply(&mut v);
        v.into_iter().map(|i| i.name).collect::<Vec<_>>()
    };
    assert_eq!(run(Filter::default()).len(), 3);
    assert_eq!(run(Filter { folder: Some("sermons".into()), ..Default::default() }), ["sunday.mp4"]);
    assert_eq!(run(Filter { folder: Some("All".into()), ..Default::default() }).len(), 3);
    assert_eq!(run(Filter { tags: names(&["EASTER"]), ..Default::default() }), ["sunday.mp4", "promo.mov"]);
    assert_eq!(run(Filter { tags: names(&["easter", "choir"]), ..Default::default() }), ["promo.mov"]);
    assert_eq!(run(Filter { query: Some("choir pro".into()), ..Default::default() }), ["promo.mov"]);
    assert_eq!(run(Filter { query: Some("BG".into()), ..Default::default() }), ["bg.png"]);
}
