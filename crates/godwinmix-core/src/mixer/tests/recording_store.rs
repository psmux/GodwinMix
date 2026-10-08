use super::*;

/// Record pressed in the UI, then the app quit and opened again. The
/// destination added beside it comes back; the recording does not start by
/// itself, because nothing in the runtime store asks for it. 0.2.2 saved it
/// there and every reopen started another file.
#[tokio::test(flavor = "multi_thread")]
async fn a_recording_is_not_saved_for_the_next_start() {
    let dir = crate::observe::tempdir("recording-store");
    let store = dir.join("godwinmix.runtime.toml");
    let mut mix = with_sources(&["cam1"]).await;
    mix.persist_runtime_to(store.clone());

    mix.add_output(&OutputConfig::bare("youtube", "rtmp://127.0.0.1:1935/live2/abcd-efgh"))
        .expect("a destination nothing is listening on still attaches");
    let mut recording = OutputConfig::bare("recording-m1x2", "record://programme");
    recording.type_id = Some("record/output".into());
    recording.params.insert("directory".into(), dir.join("files").to_string_lossy().to_string().into());
    mix.add_output(&recording).expect("the recording starts");
    assert!(mix.status().outputs.iter().any(|o| o.id == "recording-m1x2"), "the recording is running");

    let saved = std::fs::read_to_string(&store).expect("the runtime store was written");
    assert!(saved.contains("id = \"youtube\""), "the destination is saved: {saved}");
    assert!(!saved.contains("recording-m1x2"), "the recording was saved for the next start: {saved}");

    // What the next start reads: the config file beside the store.
    std::fs::write(dir.join("godwinmix.toml"), "").unwrap();
    let next = crate::config::Config::load(&dir.join("godwinmix.toml")).expect("the config loads");
    let ids: Vec<&str> = next.outputs.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["youtube"]);

    mix.remove_output(&"recording-m1x2".to_string()).expect("the recording stops");
    mix.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
