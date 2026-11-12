//! One datagram in five lost, both ways: the dump still arrives whole, every
//! command still lands once and in order, and the state the client reports
//! ends where the switcher's is.

mod fake;

use std::time::Duration;

use godwinmix_atem::{Command, Switchers, Timing};

#[tokio::test(flavor = "multi_thread")]
async fn state_converges_with_one_packet_in_five_lost() {
    let fake = fake::Fake::start(0, 5).await;
    let mut switchers = Switchers::with_timing(Timing::default().faster(10));
    let atem = switchers.add("atem-main", &fake.addr.to_string()).unwrap();
    atem.wait_up(Duration::from_secs(10)).await.unwrap();
    assert_eq!(atem.snapshot().state.inputs.len(), 6, "the whole dump, gaps resent");

    let mut sent = Vec::new();
    for round in 0..40u16 {
        let input = 1 + round % 4;
        let commands = vec![
            Command::Preview { me: 0, source: input },
            Command::Cut { me: 0 },
            Command::AuxSource { aux: 0, source: 5 - input },
        ];
        sent.extend(commands.iter().map(Command::encode));
        atem.send(commands).await.unwrap();
    }
    sent.push(Command::DskOnAir { dsk: 0, on_air: true }.encode());
    atem.send(vec![Command::DskOnAir { dsk: 0, on_air: true }]).await.unwrap();

    let got = fake.wait_blocks(sent.len()).await;
    assert_eq!(got, sent, "each command once, in the order sent");
    let truth = fake.seen.lock().unwrap().sim.clone();
    let mut watch = atem.watch();
    let s = tokio::time::timeout(
        Duration::from_secs(10),
        watch.wait_for(|s| s.state.downstream_keyers[0].on_air && s.state.program(0) == Some(truth.program)),
    )
    .await
    .expect("the client's state reaches the switcher's")
    .unwrap()
    .clone();
    assert_eq!(s.state.preview(0), Some(truth.preview));
    assert_eq!(s.state.aux, [truth.aux]);
    assert!(s.state.tally[&truth.program].program);
    assert_eq!(s.link.sessions, 1, "losses are repaired inside the session");
    fake.stop().await;
}
