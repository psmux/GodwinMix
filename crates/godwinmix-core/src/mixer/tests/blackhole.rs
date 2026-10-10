//! The cable pull of 2026-10-10, automated: an RTMP pull through a relay
//! that goes silent without closing anything.
//!
//! The source must read stalled, be restarted on the normal schedule, keep
//! being retried while the relay swallows every new connection, and come
//! back live on its own when the cable goes back in. Before 0.3.2 it read
//! `live` for the whole outage, because `livesync` went on repeating its last
//! frame into the probe that judged it.
//!
//! The RTMP server is `ffmpeg -listen 1`, started again each time a client
//! leaves, so the test is skipped where there is no ffmpeg with libx264.

use super::cable::Cable;
use super::supervision::Rig;
use super::*;
use std::process::{Child, Command as Process, Stdio};
use std::sync::atomic::AtomicBool;

/// An RTMP server for one player at a time, on `port`, for as long as the
/// returned flag stays true.
pub(super) fn serve(port: u16) -> Option<Arc<AtomicBool>> {
    let probe = Process::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status();
    if !probe.is_ok_and(|s| s.success()) {
        return None;
    }
    let running = Arc::new(AtomicBool::new(true));
    let keep = running.clone();
    std::thread::spawn(move || {
        while keep.load(Ordering::SeqCst) {
            let Ok(mut child) = launch(port) else { return };
            while keep.load(Ordering::SeqCst) && child.try_wait().ok().flatten().is_none() {
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    });
    Some(running)
}

fn launch(port: u16) -> std::io::Result<Child> {
    let url = format!("rtmp://127.0.0.1:{port}/live/far");
    Process::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-re"])
        .args(["-f", "lavfi", "-i", "testsrc=size=320x180:rate=30"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency", "-g", "30"])
        .args(["-c:a", "aac", "-f", "flv", "-listen", "1", &url])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

fn free_port() -> u16 {
    (20220..20240)
        .find(|p| std::net::TcpListener::bind(("127.0.0.1", *p)).is_ok())
        .unwrap_or_else(|| std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_whose_feed_goes_silent_is_restarted_and_comes_back() {
    let upstream = free_port();
    let Some(server) = serve(upstream) else {
        eprintln!("skipped: no ffmpeg to stand in for the RTMP server");
        return;
    };
    let cable = Cable::to(upstream, true);
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.stall.restart_after_secs = 1;
    cfg.stall.connect_timeout_secs = 3;
    let mut rig = Rig::new(cfg);
    // Give the first ffmpeg time to open its port before the first pull.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    rig.add(&format!(
        "id = \"far\"\nuri = \"rtmp://127.0.0.1:{}/live/far\"\nrtmp_client = \"rtmp2\"\nstall_timeout_secs = 1.0\n",
        cable.port
    ));
    let live = rig.until("far", SourceState::Live, Duration::from_secs(30)).await;
    if live.is_none() {
        server.store(false, Ordering::SeqCst);
        rig.mix.shutdown();
        eprintln!("skipped: the ffmpeg here would not serve an RTMP player");
        return;
    }
    let first = cable.accepted();

    cable.set(false);
    let stalled = rig.until("far", SourceState::Stalled, Duration::from_secs(10)).await;
    // Out long enough for the stall restart and at least one connect deadline.
    rig.run(Duration::from_secs(10)).await;
    let tries = cable.accepted() - first;
    let attempts = rig.mix.source_attempts.get(&SourceId::from("far")).copied().unwrap_or(0);

    cable.set(true);
    let back = rig.until("far", SourceState::Live, Duration::from_secs(40)).await;
    server.store(false, Ordering::SeqCst);
    rig.mix.shutdown();

    assert!(stalled.is_some(), "a silent feed never read stalled");
    assert!(attempts >= 1, "the stalled source was never restarted");
    assert!(tries >= 2, "only {tries} connect attempts while the cable was out");
    assert!(back.is_some(), "the source did not come back once the cable was in");
}
