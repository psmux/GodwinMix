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

/// A `FLUSH_STOP` on a compositor pad clears the pad's buffer with no lock
/// held, and landing mid cycle it frees a frame under the scaler threads
/// (`gstutil::after_next_frame`). A restart in place flushes across the proxy
/// with the slot on air, so the only safe number of flush events at that pad
/// is none: `gstutil::stop_flushes_here` on the branch's queue.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_never_flushes_the_programme_compositor() {
    let mut mix = with_sources(&["flushless"]).await;
    let id: SourceId = "flushless".into();
    mix.take(Some(id.clone()), None).unwrap();
    let pad = mix.pool.slots().iter().find(|s| s.source() == Some(&id)).unwrap().pad().clone();
    let flushes = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let frames = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let (saw_flush, saw_frame) = (flushes.clone(), frames.clone());
    let kinds = gst::PadProbeType::EVENT_FLUSH | gst::PadProbeType::EVENT_DOWNSTREAM | gst::PadProbeType::BUFFER;
    pad.add_probe(kinds, move |_, info| {
        match info.event().map(|e| e.type_()) {
            Some(gst::EventType::FlushStart | gst::EventType::FlushStop) => {
                saw_flush.fetch_add(1, Ordering::Relaxed);
            }
            None => {
                saw_frame.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        gst::PadProbeReturn::Ok
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    for _ in 0..3 {
        mix.handle(Command::RestartSource(id.clone())).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let before = frames.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let after = frames.load(Ordering::Relaxed);
    // Read before the shutdown, which unbinds every slot and flushes each one
    // by the slot pool's own hidden, one frame later route.
    let reached = flushes.load(Ordering::Relaxed);
    mix.shutdown();
    assert_eq!(reached, 0, "a restart's flush reached the programme compositor");
    assert!(after > before + 10, "the picture did not come back: {before} then {after}");
}
