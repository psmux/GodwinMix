use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn restarting_a_static_source_restores_programme_media_and_status() {
    let mut mix = with_sources(&["restarting"]).await;
    let id: SourceId = "restarting".into();
    mix.take(Some(id.clone()), None).unwrap();
    let pad = mix.pool.slots().iter().find(|s| s.source() == Some(&id)).unwrap().pad().clone();
    let frames = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counted = frames.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(frames.load(Ordering::Relaxed) > 0);
    let mut received = Vec::new();
    let mut media_present = true;
    for _ in 0..3 {
        mix.handle(Command::RestartSource(id.clone())).unwrap();
        let before = frames.load(Ordering::Relaxed);
        tokio::time::sleep(Duration::from_secs(2)).await;
        received.push(frames.load(Ordering::Relaxed) - before);
        let source = mix.sources.iter().find(|s| s.input.id == id).unwrap();
        media_present &= source.input.has_video() && source.input.has_audio();
    }
    mix.shutdown();
    assert!(received.iter().all(|n| *n > 10), "restarted programme frame counts: {received:?}");
    assert!(media_present, "restarted source media disappeared from status");
}
