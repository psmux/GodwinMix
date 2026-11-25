//! The switcher goes away and comes back: the client says why it is down,
//! keeps trying by itself, and comes up on a new session with fresh state.

mod fake;

use std::time::Duration;

use godwinmix_atem::session::reasons;
use godwinmix_atem::{Command, LinkStatus, Switchers, Timing};

#[tokio::test(flavor = "multi_thread")]
async fn the_client_comes_back_after_the_switcher_restarts() {
    let fake = fake::Fake::start(0, 0).await;
    let addr = fake.addr;
    let mut switchers = Switchers::with_timing(Timing::default().faster(10));
    let atem = switchers.add("atem-main", &addr.to_string()).unwrap();
    atem.wait_up(Duration::from_secs(5)).await.unwrap();
    atem.send(vec![Command::Program { me: 0, source: 4 }]).await.unwrap();
    fake.wait_blocks(1).await;
    fake.stop().await;

    let mut watch = atem.watch();
    let down = tokio::time::timeout(Duration::from_secs(5), watch.wait_for(|s| s.link.status == LinkStatus::Connecting))
        .await
        .expect("the silence is noticed")
        .unwrap()
        .clone();
    let why = down.link.reason.clone().unwrap_or_default();
    assert!([reasons::SILENT, reasons::REFUSED, reasons::NO_ANSWER].contains(&why.as_str()), "{why}");
    let refused = atem.send(vec![Command::Cut { me: 0 }]).await.unwrap_err();
    assert_eq!(refused.code(), "switcher_down");

    let fake = fake::Fake::start(addr.port(), 0).await;
    atem.wait_up(Duration::from_secs(10)).await.unwrap();
    let s = atem.snapshot();
    assert_eq!(s.link.sessions, 2);
    assert_eq!(s.link.reason, None);
    assert_eq!(s.state.program(0), Some(1), "the new dump, not the old state");
    atem.send(vec![Command::Cut { me: 0 }]).await.unwrap();
    assert_eq!(fake.wait_blocks(1).await[0][4..8], *b"DCut");
    fake.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_listening_is_reported_while_trying() {
    // Bind and free a port so that nothing listens on it.
    let port = std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut switchers = Switchers::with_timing(Timing::default().faster(10));
    let atem = switchers.add("atem-gone", &format!("127.0.0.1:{port}")).unwrap();
    let mut watch = atem.watch();
    let s = tokio::time::timeout(Duration::from_secs(5), watch.wait_for(|s| s.link.reason.is_some()))
        .await
        .expect("a reason is given")
        .unwrap()
        .clone();
    let why = s.link.reason.unwrap();
    // Windows and Linux report the ICMP; a system that does not still says
    // there was no answer.
    assert!(why == reasons::REFUSED || why == reasons::NO_ANSWER, "{why}");
    assert_eq!(atem.snapshot().link.status, LinkStatus::Connecting);
}
