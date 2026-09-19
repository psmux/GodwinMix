use super::*;

/// Run separately so other real pipelines cannot contaminate wall clock timing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "measures programme timing over repeated source retirement"]
async fn source_churn_keeps_the_sixty_frame_programme_budget() {
    let mut mix = with_sources(&["on-air"]).await;
    mix.take(Some("on-air".into()), None).unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    crate::observe::metrics::reset_longest_frame_gap();
    for n in 0..12 {
        let cfg = SourceConfig::bare(&format!("temporary-{n}"), "test://ball");
        mix.add_source(&cfg, None).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        mix.remove_source(&cfg.id).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    let worst = crate::observe::metrics::worst_frame_stall();
    mix.shutdown();
    eprintln!("worst sixty frame average during source churn: {worst:?}");
    assert!(worst <= Duration::from_millis(34), "source churn averaged {worst:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_retired_slot_ends_its_input_and_accepts_the_next_source() {
    let mut mix = with_sources(&["retiring"]).await;
    let old: SourceId = "retiring".into();
    let slot = mix.pool.slots().iter().find(|s| s.source() == Some(&old)).unwrap().index;
    let pad = mix.pool.slots()[slot].pad().clone();
    mix.remove_source(&old).unwrap();
    let ended = pad.pad_flags().contains(gst::PadFlags::EOS);
    let cfg = SourceConfig::bare("replacement", "test://ball");
    mix.add_source(&cfg, None).unwrap();
    for _ in 0..100 {
        if !pad.pad_flags().contains(gst::PadFlags::EOS) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let resumed = !pad.pad_flags().contains(gst::PadFlags::EOS);
    let rebound = mix.pool.slots()[slot].source() == Some(&cfg.id);
    mix.shutdown();
    assert!(ended, "an empty compositor slot still waits for a frame");
    assert!(rebound && resumed, "a reused compositor slot did not resume");
}
