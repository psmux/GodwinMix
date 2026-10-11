//! A source's backup when the source has never connected, against a real
//! pipeline and a relay that takes the connection and never answers.

use super::*;

/// A camera that never connects at all, behind a relay that takes the
/// connection and never answers, so it reads `connecting` for good: its
/// backup goes on once the connect timeout has passed, and the status says
/// it has not connected rather than that it was lost.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_that_never_connects_is_drawn_as_its_backup_after_the_connect_timeout() {
    let dead = super::super::cable::Cable::to(9, false);
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.stall.restart_after_secs = 600;
    cfg.stall.connect_timeout_secs = 2;
    let mut rig = Rig::new(cfg);
    add(&mut rig, "wide", "smpte", "");
    live_all(&mut rig, &["wide"]).await;
    rig.add(&format!(
        "id = \"main\"\nuri = \"rtmp://127.0.0.1:{}/live/main\"\nrtmp_client = \"rtmp2\"\n\
         on_loss = {{ backup = \"wide\", after_secs = 0, transition = \"cut\" }}\n",
        dead.port
    ));
    let added = Instant::now();
    rig.mix.take(Some("main".into()), None).unwrap();
    rig.run(Duration::from_millis(1200)).await;
    let early = drawn(&rig);
    let frames = count_programme(&rig.mix);
    let (on, _) = until(&mut rig, &frames, Duration::from_secs(15), |r| drawn(r) == ["wide"]).await;
    let waited = added.elapsed();
    let main = rig.mix.status().sources.into_iter().find(|s| s.id == "main").unwrap();
    let state = rig.state("main");
    rig.mix.shutdown();

    assert!(early.is_empty(), "the backup went on inside the connect timeout: drawing {early:?}");
    assert!(on.is_some(), "the backup never went on for a source that never connected; it reads {state:?}");
    assert!(waited >= Duration::from_secs(2), "switched after {waited:?}, inside the connect timeout");
    assert_eq!(state, Some(SourceState::Connecting), "the relay was meant to keep it connecting");
    let backup = main.backup.expect("a backup status");
    assert_eq!(backup.phase, godwinmix_protocol::backup::BackupPhase::OnBackup);
    assert!(backup.never_live, "the status said lost for a source that never connected");
    assert_eq!(backup.showing.as_deref(), Some("wide"));
}

/// A camera whose server is not up at all, so the connection is refused and
/// the source never starts: while it waits to be tried again it is listed as
/// failed rather than missing, and its backup goes on where it is placed.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_that_will_not_start_is_listed_and_drawn_as_its_backup() {
    let refused = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.stall.restart_after_secs = 600;
    // Five seconds, so a loaded runner's slow second does not put the
    // early look past it.
    cfg.stall.connect_timeout_secs = 5;
    let mut rig = Rig::with_bus(cfg);
    add(&mut rig, "wide", "smpte", "");
    live_all(&mut rig, &["wide"]).await;
    let main: SourceConfig = toml::from_str(&format!(
        "id = \"main\"\nuri = \"rtmp://127.0.0.1:{refused}/live/main\"\n\
         on_loss = {{ backup = \"wide\", after_secs = 0, transition = \"cut\" }}\n"
    ))
    .unwrap();
    // Where the core's log left it: the start refused, the source removed,
    // and a try armed for later. Each try is a real pipeline against the
    // refused address.
    let refusal = Err(anyhow::anyhow!("Could not open resource for reading."));
    rig.mix.note_unstarted(&main, &refusal, true);
    let canvas = rig.mix.canvas().clone();
    rig.mix.take_scene(scene("cam", vec![Placement::full_canvas("main".into(), &canvas)]), None).unwrap();
    let listed = rig.mix.status().sources.into_iter().find(|s| s.id == "main");
    rig.run(Duration::from_millis(1200)).await;
    let early = drawn(&rig);
    let frames = count_programme(&rig.mix);
    let (on, gap) = until(&mut rig, &frames, Duration::from_secs(15), |r| drawn(r) == ["wide"]).await;
    let after = rig.mix.status().sources.into_iter().find(|s| s.id == "main");
    rig.mix.shutdown();

    let listed = listed.expect("a source that would not start was missing from the status");
    assert_eq!(listed.state, SourceState::Failed, "listed, but not as failed");
    assert!(early.is_empty(), "the backup went on inside the connect timeout: drawing {early:?}");
    assert!(on.is_some(), "the backup never went on for a source that would not start");
    assert!(gap < STALL, "the programme stalled for {gap:?}");
    let backup = after.and_then(|s| s.backup).expect("a backup status");
    assert_eq!(backup.phase, godwinmix_protocol::backup::BackupPhase::OnBackup);
    assert!(backup.never_live, "the status said lost for a source that never started");
}
