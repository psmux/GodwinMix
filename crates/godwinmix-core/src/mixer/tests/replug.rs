//! The cable pull of 2026-10-10 played to its end: after the replug a pull
//! source comes back live, stays live, and has its picture beside its sound.
//!
//! The restarted RTMP pull came back 35 seconds into the show, read live for
//! three seconds and then stalled again for fifteen. `livesync` had stamped
//! its picture on the programme's clock while its sound kept its own
//! timeline from zero, and the aligner added the programme's running time to
//! both: the picture was placed half a minute ahead. See
//! `plugin::kinds::livesync_clock`. The programme here is left up for a few
//! seconds before anything is added, because a programme at zero hides it.

use super::cable::Cable;
use super::supervision::Rig;
use super::*;

/// Where the source's last picture and last sound sit on its own timeline,
/// and how far ahead of the programme its picture is placed, in ms.
fn placement(rig: &Rig, id: &str) -> Option<(i64, i64)> {
    let slot = rig.slot(id)?;
    let video = slot.input.last_video.running()?.nseconds() as i64;
    let audio = slot.input.last_audio.running()?.nseconds() as i64;
    let aligner = slot.aligner.as_ref()?;
    let now = rig.mix.running_time()?.nseconds() as i64;
    let ahead = video + aligner.offset()? - aligner.catch.total() - now;
    Some(((video - audio) / 1_000_000, ahead / 1_000_000))
}

/// Pull the cable under `uri` (which goes through `cable`) once the source is
/// live, put it back once the source has been restarted into the dead relay,
/// and watch for twelve seconds after it is live again. `lip` also holds the
/// last picture and the last sound to within two seconds of each other on the
/// source's own timeline; the RTSP test camera's sound can start seconds off
/// its picture by itself on a loaded machine, so it is not held to that.
async fn pull_and_replug(cable: &Cable, uri: &str, lip: bool) -> Option<()> {
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.stall.restart_after_secs = 1;
    cfg.stall.connect_timeout_secs = 3;
    // Software decoding, so the picture is decoded the same on every machine.
    cfg.hardware.decode = crate::config::Accel::Software;
    cfg.hardware.encode = crate::config::Accel::Software;
    let mut rig = Rig::new(cfg);
    rig.run(Duration::from_secs(6)).await;
    rig.add(&format!("id = \"far\"\nuri = \"{uri}\"\nstall_timeout_secs = 1.0\n"));
    rig.mix.take(Some("far".into()), None).unwrap();
    if rig.until("far", SourceState::Live, Duration::from_secs(30)).await.is_none() {
        rig.mix.shutdown();
        eprintln!("skipped: {uri} never went live here");
        return None;
    }
    cable.set(false);
    let stalled = rig.until("far", SourceState::Stalled, Duration::from_secs(10)).await;
    rig.run(Duration::from_secs(8)).await;
    let restarts = rig.mix.source_attempts.get(&SourceId::from("far")).copied().unwrap_or(0);
    cable.set(true);
    let back = rig.until("far", SourceState::Live, Duration::from_secs(40)).await;

    let (mut states, mut worst) = (vec![], (0i64, 0i64));
    for _ in 0..24 {
        rig.run(Duration::from_millis(500)).await;
        let state = rig.state("far");
        if states.last() != Some(&state) {
            states.push(state);
        }
        if let Some((apart, ahead)) = placement(&rig, "far") {
            worst = (worst.0.max(apart.abs()), worst.1.max(ahead.abs()));
        }
    }
    rig.mix.shutdown();

    assert!(stalled.is_some(), "{uri}: the pulled cable never read stalled");
    assert!(restarts >= 1, "{uri}: the stalled source was never restarted");
    assert!(back.is_some(), "{uri}: the source did not come back after the replug");
    assert_eq!(states, vec![Some(SourceState::Live)], "{uri}: it left live after coming back");
    // The programme's mixers run a second behind the clock, so a picture a
    // second either side of now is still drawn; the fault put it as far ahead
    // as the programme was old.
    assert!(worst.1 < 1000, "{uri}: the picture was placed {} ms off the programme's time", worst.1);
    assert!(!lip || worst.0 < 2000, "{uri}: picture and sound were {} ms apart on the source's timeline", worst.0);
    Some(())
}

fn free_port() -> u16 {
    (21040..21060)
        .find(|p| std::net::TcpListener::bind(("127.0.0.1", *p)).is_ok())
        .unwrap_or_else(|| std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_rtsp_camera_comes_back_from_a_pulled_cable_and_stays_on_time() {
    let port = free_port();
    let Some(_cam) = super::rtsp_live::Camera::serve(port) else { return };
    let cable = Cable::to(port, true);
    pull_and_replug(&cable, &format!("rtspt://127.0.0.1:{}/cam", cable.port), false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_rtmp_pull_comes_back_from_a_pulled_cable_and_stays_on_time() {
    let upstream = free_port();
    let Some(server) = super::blackhole::serve(upstream) else {
        eprintln!("skipped: no ffmpeg to stand in for the RTMP server");
        return;
    };
    // Give the first ffmpeg time to open its port before the first pull.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let cable = Cable::to(upstream, true);
    pull_and_replug(&cable, &format!("rtmp://127.0.0.1:{}/live/far", cable.port), true).await;
    server.store(false, Ordering::SeqCst);
}
