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
    assert!(worst <= Duration::from_millis(34), "source churn averaged {worst:?}");
}
