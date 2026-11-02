//! A guest who joins a show that has been on for a while. The programme's
//! running time is then well ahead of zero, which is what a return pipeline
//! with a base time of its own judged its buffers against: on 0.3.24 the
//! return connected and then sent nothing for as long as the show had been
//! up. The test above joins a show a second old, which hid it.

use crate::core::Core;
use crate::{join, post, rtc};
use serde_json::json;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// How long the show is up before the guest joins. Longer than `FIRST`, so a
/// return that waits as long as the show has been up cannot pass.
const SHOW_UP: Duration = Duration::from_secs(8);

/// The longest from the answer to the first decoded frame of the return.
const FIRST: Duration = Duration::from_secs(4);

/// Offer to the return until it is there, answer, and say how long the first
/// frame took after the answer.
async fn watch(base: &str, id: &str, key: &str) -> (rtc::Peer, Duration) {
    let viewer = rtc::viewer();
    let offer = viewer.offer();
    let until = Instant::now() + Duration::from_secs(15);
    let (status, _, answer) = loop {
        let got = post(&format!("{base}/guest/{id}/whep"), key, offer.clone()).await;
        if got.0 != 503 || Instant::now() > until {
            break got;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    };
    assert_eq!(status, 201, "the return was refused: {answer}");
    viewer.answer(&answer);
    let answered = Instant::now();
    while viewer.frames.load(Ordering::Relaxed) == 0 && answered.elapsed() < Duration::from_secs(20) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    (viewer, answered.elapsed())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guest_joining_a_show_that_has_been_up_a_while_sees_and_hears_the_return_at_once_and_after_a_take_and_a_rejoin() {
    if !rtc::ready() {
        return;
    }
    let core = Core::start("late").await;
    let base = core.serve_http().await;
    core.ok("source.add", json!({ "id": "bars", "uri": "test://smpte" })).await;
    core.ok("program.take", json!({ "source": "bars" })).await;
    tokio::time::sleep(SHOW_UP).await;
    let made = core.ok("guest.invite.create", json!({ "name": "Ola", "id": "ola" })).await;
    let key = made["link"].as_str().unwrap().split("key=").nth(1).unwrap().split('&').next().unwrap().to_string();

    let guest = join(&core, &base, "ola", &key).await;
    let (viewer, took) = watch(&base, "ola", &key).await;
    assert!(took < FIRST, "the first frame of the return came {took:?} after the answer, on a show up for {SHOW_UP:?}");
    let (frames, sound) = viewer.wait_for(30, 25, 10).await;
    assert!(frames >= 30 && sound >= 25, "the return decoded {frames} frames and {sound} Opus packets");

    core.ok("program.take", json!({ "source": "guest-ola" })).await;
    let before = viewer.frames.load(Ordering::Relaxed);
    let (after, _) = viewer.wait_for(before + 30, 0, 10).await;
    assert!(after >= before + 30, "the return stopped at a take: {before} then {after}");

    // The guest closes the page and opens the link again: a new publisher
    // and a new player on the same return.
    drop(viewer);
    drop(guest);
    let _guest = join(&core, &base, "ola", &key).await;
    let (again, took) = watch(&base, "ola", &key).await;
    assert!(took < FIRST, "after a rejoin the first frame came {took:?} after the answer");
    let (frames, sound) = again.wait_for(30, 25, 10).await;
    assert!(frames >= 30 && sound >= 25, "after a rejoin the return decoded {frames} frames and {sound} Opus packets");
}
