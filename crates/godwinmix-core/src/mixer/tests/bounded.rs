use super::*;

/// Every queue in a running show has a byte and a buffer limit: the
/// programme with sources on air and in the mosaic, a source's own pipeline,
/// and a recording. A queue with a time limit alone is what grew a show to
/// 11.8 GB when the timestamps through it stopped counting.
#[tokio::test(flavor = "multi_thread")]
async fn every_queue_in_a_running_show_has_a_byte_and_a_buffer_cap() {
    let dir = crate::observe::tempdir("bounded-queues");
    let mut mix = with_sources_and_mosaic(&["cam1", "cam2"]).await;
    mix.take(Some("cam1".into()), None).expect("the take");
    let mut recording = OutputConfig::bare("bounded-rec", "record://programme");
    recording.type_id = Some("record/output".into());
    recording.params.insert("directory".into(), dir.to_string_lossy().to_string().into());
    mix.add_output(&recording).expect("the recording starts");
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // This show's own pipelines, not the registry's: tests running beside
    // this one register pipelines under the same names.
    let mut mine: Vec<gst::Pipeline> = vec![mix.program.clone()];
    mine.extend(mix.sources.iter().map(|s| s.input.pipeline.clone()));
    mine.extend(crate::observe::introspect::pipeline("output-bounded-rec"));
    let mut seen = 0;
    let mut open = Vec::new();
    for pipeline in &mine {
        let name = pipeline.name();
        for element in pipeline.iterate_recurse().into_iter().flatten() {
            if element.factory().is_none_or(|f| f.name() != "queue") {
                continue;
            }
            seen += 1;
            let bytes = element.property::<u32>("max-size-bytes");
            let buffers = element.property::<u32>("max-size-buffers");
            if bytes == 0 || buffers == 0 {
                open.push(format!("{name}/{} (bytes {bytes}, buffers {buffers})", element.name()));
            }
        }
    }
    mix.remove_output(&"bounded-rec".to_string()).ok();
    mix.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(seen >= 10, "only {seen} queues were found; the show did not build what this test reads");
    assert!(open.is_empty(), "queues with no byte or buffer limit: {open:?}");
}

/// The guard raises its alarm past the threshold and clears it under, on its
/// own task, with the mixer thread only starting the check.
#[tokio::test(flavor = "multi_thread")]
async fn the_memory_guard_raises_and_clears_its_alarm() {
    let mut mix = with_sources(&["cam1"]).await;
    mix.cfg.memory.guard_mb = Some(1);
    mix.guard_memory();
    let mut raised = None;
    for _ in 0..100 {
        raised = mix.handle.memory_alarm();
        if raised.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let raised = raised.expect("a show past a one megabyte guard raised no alarm");
    assert!(raised.detail.contains("past its 1 MB guard"), "{}", raised.detail);

    mix.cfg.memory.guard_mb = Some(1_000_000);
    mix.memory.guard.due = None;
    // The check before may still be finishing; the next one starts after it.
    let mut cleared = false;
    for _ in 0..100 {
        mix.guard_memory();
        mix.memory.guard.due = None;
        if mix.handle.memory_alarm().is_none() {
            cleared = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    mix.shutdown();
    assert!(cleared, "the alarm stayed up under a guard the show is far below");
}
