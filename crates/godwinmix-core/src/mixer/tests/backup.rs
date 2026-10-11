//! A source's backup, against real pipelines: a test source is paused to
//! take its picture away and played again to bring it back.

use super::full_pool::count_programme;
use super::supervision::Rig;
use super::*;
use crate::mixer::failover::BackupCommand;

/// The longest the programme may go without a frame before a test calls it a
/// stall, sampled every tenth of a second off a loaded runner.
const STALL: Duration = Duration::from_secs(2);

fn rig() -> Rig {
    let mut cfg = programme_config(crate::config::Accel::Software);
    // The supervisor's own restart would bring the paused source back by
    // itself; these tests bring it back by hand, when they mean to.
    cfg.stall.restart_after_secs = 600;
    Rig::new(cfg)
}

/// A ball with a stall limit short enough for a test, and a rule if given.
fn add(rig: &mut Rig, id: &str, pattern: &str, on_loss: &str) {
    let rule = if on_loss.is_empty() { String::new() } else { format!("on_loss = {{ {on_loss} }}\n") };
    rig.add(&format!("id = \"{id}\"\nuri = \"test://{pattern}\"\nstall_timeout_secs = 0.5\n{rule}"));
}

fn drawn(rig: &Rig) -> Vec<String> {
    rig.mix.current_placements().iter().map(|p| p.source.to_string()).collect()
}

fn set_playing(rig: &Rig, id: &str, playing: bool) {
    let input = rig.slot(id).expect("the source").input.clone();
    let state = if playing { gst::State::Playing } else { gst::State::Paused };
    let _ = input.pipeline.set_state(state);
}

async fn live_all(rig: &mut Rig, ids: &[&str]) {
    for id in ids {
        assert!(rig.until(id, SourceState::Live, Duration::from_secs(15)).await.is_some(), "{id} never went live");
    }
    // Two ticks while live, so each is written down as last seen live.
    rig.run(TICK * 2).await;
}

/// Run until `want` holds, sampling the programme's frame count as it goes.
/// Answers how long it took and the longest the programme went without a
/// frame meanwhile.
async fn until(rig: &mut Rig, frames: &AtomicU64, within: Duration, want: impl Fn(&Rig) -> bool) -> (Option<Duration>, Duration) {
    let started = Instant::now();
    let (mut last, mut since, mut worst) = (frames.load(Ordering::Relaxed), Instant::now(), Duration::ZERO);
    while started.elapsed() < within {
        if want(rig) {
            return (Some(started.elapsed()), worst);
        }
        rig.run(Duration::from_millis(100)).await;
        let now = frames.load(Ordering::Relaxed);
        if now != last {
            (last, since) = (now, Instant::now());
        }
        worst = worst.max(since.elapsed());
    }
    (None, worst)
}

/// The whole story with an automatic return: held, swapped over a fade,
/// back once steady, and the programme never short of a frame.
#[tokio::test(flavor = "multi_thread")]
async fn a_lost_source_is_drawn_as_its_backup_and_returns_once_steady() {
    let mut rig = rig();
    add(&mut rig, "wide", "smpte", "");
    add(&mut rig, "main", "ball", "backup = \"wide\", after_secs = 1, transition = \"dissolve\", duration_ms = 400, return_after_secs = 2");
    live_all(&mut rig, &["wide", "main"]).await;
    rig.mix.take(Some("main".into()), None).unwrap();
    let frames = count_programme(&rig.mix);
    let mut events = rig.mix.handle.subscribe();

    set_playing(&rig, "main", false);
    let lost = Instant::now();
    let (swapped, gap_out) = until(&mut rig, &frames, Duration::from_secs(10), |r| drawn(r) == ["wide"]).await;
    let held_for = lost.elapsed();
    let faded = rig.mix.running_transition.as_ref().map(|t| t.kind.clone());
    let status = rig.mix.status();
    let main = status.sources.iter().find(|s| s.id == "main").unwrap().clone();

    set_playing(&rig, "main", true);
    let back_at = Instant::now();
    let (returned, gap_back) = until(&mut rig, &frames, Duration::from_secs(15), |r| drawn(r) == ["main"]).await;
    let waited = back_at.elapsed();
    let mut names = Vec::new();
    while let Ok(e) = events.try_recv() {
        if let Some((name, _)) = godwinmix_protocol::rpc::event_name_and_payload(&e.event) {
            names.push(name);
        }
    }
    rig.mix.shutdown();

    assert!(swapped.is_some(), "the backup never went on: drawing {:?}", drawn(&rig));
    assert!(held_for >= Duration::from_millis(1400), "switched after {held_for:?}, before the stall and the hold");
    assert_eq!(faded.as_deref(), Some("fade"), "the swap on air was not the rule's dissolve");
    assert_eq!(main.backup.as_ref().map(|b| b.phase), Some(godwinmix_protocol::backup::BackupPhase::OnBackup));
    assert_eq!(main.backup.and_then(|b| b.showing).as_deref(), Some("wide"));
    assert!(returned.is_some(), "main never came back on air");
    assert!(waited >= Duration::from_secs(2), "returned after {waited:?}, before it had been steady for 2 s");
    assert!(names.contains(&"source.backup.on") && names.contains(&"source.backup.off"), "{names:?}");
    // Two seconds, as the replug test allows: a loaded Linux runner went
    // 1.02 s between two samples of the frame count while the swap ran.
    assert!(gap_out < STALL && gap_back < STALL, "the programme stalled: {gap_out:?} {gap_back:?}");
}

/// `return = "manual"` stays on the backup after the source is back, until
/// somebody returns it, and a return before it is back is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_manual_return_waits_for_the_operator() {
    let mut rig = rig();
    add(&mut rig, "wide", "smpte", "");
    add(&mut rig, "main", "ball", "backup = \"wide\", after_secs = 0, return = \"manual\", return_after_secs = 0");
    live_all(&mut rig, &["wide", "main"]).await;
    rig.mix.take(Some("main".into()), None).unwrap();
    let frames = count_programme(&rig.mix);
    set_playing(&rig, "main", false);
    let (on, _) = until(&mut rig, &frames, Duration::from_secs(10), |r| drawn(r) == ["wide"]).await;
    let early = rig.mix.backup_command(BackupCommand::Return("main".into()));
    set_playing(&rig, "main", true);
    assert!(rig.until("main", SourceState::Live, Duration::from_secs(10)).await.is_some());
    rig.run(Duration::from_secs(3)).await;
    let still = drawn(&rig);
    rig.mix.backup_command(BackupCommand::Return("main".into())).expect("the return");
    let (back, _) = until(&mut rig, &frames, Duration::from_secs(5), |r| drawn(r) == ["main"]).await;
    rig.mix.shutdown();

    assert!(on.is_some(), "the backup never went on");
    let early = early.expect_err("a return while main had no picture was let through");
    assert!(early.to_string().contains("no picture yet"), "{early}");
    assert_eq!(still, ["wide"], "a manual return went back by itself");
    assert!(back.is_some(), "the operator's return did nothing");
}

/// The backup down too: its own backup stands in.
#[tokio::test(flavor = "multi_thread")]
async fn a_backup_that_is_down_too_hands_on_to_its_own_backup() {
    let mut rig = rig();
    add(&mut rig, "clip", "snow", "");
    add(&mut rig, "wide", "smpte", "backup = \"clip\", after_secs = 0");
    add(&mut rig, "main", "ball", "backup = \"wide\", after_secs = 0");
    live_all(&mut rig, &["clip", "wide", "main"]).await;
    rig.mix.take(Some("main".into()), None).unwrap();
    let frames = count_programme(&rig.mix);
    set_playing(&rig, "wide", false);
    assert!(rig.until("wide", SourceState::Stalled, Duration::from_secs(10)).await.is_some());
    rig.run(Duration::from_secs(3)).await;
    set_playing(&rig, "main", false);
    let (on, gap) = until(&mut rig, &frames, Duration::from_secs(10), |r| drawn(r) == ["clip"]).await;
    let seen = drawn(&rig);
    rig.mix.shutdown();
    assert!(on.is_some(), "the chain did not reach clip: drawing {seen:?}");
    assert!(gap < STALL, "the programme stalled for {gap:?}");
}

/// A take the operator makes while a backup is on is the take that happens,
/// and the source coming back does not undo it.
#[tokio::test(flavor = "multi_thread")]
async fn the_operators_take_is_left_alone() {
    let mut rig = rig();
    add(&mut rig, "wide", "smpte", "");
    add(&mut rig, "other", "snow", "");
    add(&mut rig, "main", "ball", "backup = \"wide\", after_secs = 0, return_after_secs = 0");
    live_all(&mut rig, &["wide", "other", "main"]).await;
    rig.mix.take(Some("main".into()), None).unwrap();
    let frames = count_programme(&rig.mix);
    set_playing(&rig, "main", false);
    let (on, _) = until(&mut rig, &frames, Duration::from_secs(10), |r| drawn(r) == ["wide"]).await;
    rig.mix.take(Some("other".into()), None).unwrap();
    set_playing(&rig, "main", true);
    let (back, _) = until(&mut rig, &frames, Duration::from_secs(10), |r| !r.mix.backups.engaged("main")).await;
    rig.run(Duration::from_secs(1)).await;
    let after = drawn(&rig);
    let program = rig.mix.program_source.clone();
    rig.mix.shutdown();
    assert!(on.is_some() && back.is_some(), "the backup never went on and off");
    assert_eq!(after, ["other"], "the return fought the operator's take");
    assert_eq!(program.as_deref(), Some("other"));
}

/// A picture in picture draws the backup in the inset, where the source was.
#[tokio::test(flavor = "multi_thread")]
async fn the_backup_takes_the_insets_place() {
    let mut rig = rig();
    add(&mut rig, "wide", "smpte", "");
    add(&mut rig, "other", "snow", "");
    add(&mut rig, "main", "ball", "backup = \"wide\", after_secs = 0, transition = \"cut\"");
    live_all(&mut rig, &["wide", "other", "main"]).await;
    let canvas = rig.mix.canvas().clone();
    let inset = Placement { xpos: 160, ypos: 90, width: 160, height: 90, ..Placement::full_canvas("main".into(), &canvas) };
    rig.mix.take_scene(scene("pip", vec![Placement::full_canvas("other".into(), &canvas), inset]), None).unwrap();
    let frames = count_programme(&rig.mix);
    set_playing(&rig, "main", false);
    let (on, _) = until(&mut rig, &frames, Duration::from_secs(10), |r| drawn(r) == ["other", "wide"]).await;
    let placed = rig.mix.current_placements();
    rig.mix.shutdown();
    assert!(on.is_some(), "the inset was not taken over: {:?}", placed.iter().map(|p| &p.source).collect::<Vec<_>>());
    assert_eq!((placed[1].xpos, placed[1].width), (160, 160), "the backup was not drawn where the inset was");
}

/// A loop of backups is refused when it is set, and nothing changes.
#[tokio::test(flavor = "multi_thread")]
async fn a_loop_of_backups_is_refused() {
    let mut rig = rig();
    add(&mut rig, "wide", "smpte", "");
    add(&mut rig, "main", "ball", "backup = \"wide\"");
    let looped = rig.mix.backup_command(BackupCommand::Set("wide".into(), Some(godwinmix_protocol::backup::OnLoss::to("main"))));
    let unknown = rig.mix.backup_command(BackupCommand::Set("main".into(), Some(godwinmix_protocol::backup::OnLoss::to("nope"))));
    let rule = rig.mix.backups.rule("main").map(|r| r.backup.clone());
    rig.mix.shutdown();
    assert!(looped.unwrap_err().to_string().contains("wide -> main -> wide"));
    assert!(unknown.unwrap_err().to_string().contains("no source called nope"));
    assert_eq!(rule.as_deref(), Some("wide"), "a refused change changed the rule");
}

/// A camera that never connects at all, behind a relay that takes the
/// connection and never answers, so it reads `connecting` for good: its
/// backup goes on once the connect timeout has passed, and the status says
/// it has not connected rather than that it was lost.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_that_never_connects_is_drawn_as_its_backup_after_the_connect_timeout() {
    let dead = super::cable::Cable::to(9, false);
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
