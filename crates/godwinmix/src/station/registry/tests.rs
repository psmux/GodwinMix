use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-registry-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_bare_config_is_one_show_called_main_run_in_place_and_nothing_is_written() {
    let dir = scratch("bare");
    let config = dir.join("godwinmix.toml");
    std::fs::write(&config, "").unwrap();
    let reg = Registry::open(&config).unwrap();
    assert_eq!(reg.ids(), vec!["main"]);
    assert_eq!(reg.config_of(&reg.records[0]), std::path::absolute(&config).unwrap());
    reg.save().unwrap();
    assert!(!dir.join("shows.json").exists(), "one show needs no list");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_second_show_is_written_down_and_read_back_with_its_own_folder() {
    let dir = scratch("second");
    let config = dir.join("godwinmix.toml");
    let mut reg = Registry::open(&config).unwrap();
    let id = reg.free_id("Second room");
    assert_eq!(id, "second-room");
    let folder = reg.folder_for(&id);
    reg.records.push(Record::new(&id, "Second room", Some(folder.join("godwinmix.toml"))));
    reg.save().unwrap();
    let again = Registry::open(&config).unwrap();
    assert_eq!(again.ids(), vec!["main", "second-room"]);
    assert_eq!(again.free_id("Second room"), "second-room-2");
    assert_eq!(again.config_of(again.get("second-room").unwrap()), dir.join("shows/second-room/godwinmix.toml"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_list_that_will_not_parse_is_refused_and_not_written_over() {
    let dir = scratch("broken");
    std::fs::write(dir.join("shows.json"), "{ not json").unwrap();
    let err = Registry::open(&dir.join("godwinmix.toml")).unwrap_err().to_string();
    assert!(err.contains("Move it aside"), "{err}");
    assert_eq!(std::fs::read_to_string(dir.join("shows.json")).unwrap(), "{ not json");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_list_from_before_wave_four_composites_and_a_direct_show_reads_back_as_it_was() {
    let dir = scratch("wave4");
    let old = r#"{"shows": [{"id": "main", "name": "Main"}, {"id": "b", "name": "B", "config": "shows/b/godwinmix.toml"}]}"#;
    std::fs::write(dir.join("shows.json"), old).unwrap();
    let config = dir.join("godwinmix.toml");
    let mut reg = Registry::open(&config).unwrap();
    assert!(reg.records.iter().all(|r| r.compositing && r.input.is_none() && r.outputs.is_empty()));
    let mut feed = Record::new("feed", "Feed", None);
    feed.compositing = false;
    feed.input = Some(godwinmix_protocol::shows::InputSpec { uri: "udp://@239.1.1.1:5000".into(), program: Some(7), params: None, backup: None });
    reg.records.push(feed.clone());
    reg.save().unwrap();
    let text = std::fs::read_to_string(dir.join("shows.json")).unwrap();
    assert_eq!(text.matches("compositing").count(), 1, "only the show that differs says so: {text}");
    assert_eq!(Registry::open(&config).unwrap().get("feed"), Some(&feed));
    let _ = std::fs::remove_dir_all(&dir);
}
