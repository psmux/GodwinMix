//! The client against a fake switcher on loopback: the handshake, the state
//! dump read in, and the exact bytes of every command as they arrive.

mod fake;

use std::time::Duration;

use godwinmix_atem::wire::Flags;
use godwinmix_atem::{Command, RateStyle, Switchers, Timing, TransitionStyle};

fn fast() -> Timing {
    Timing::default().faster(10)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_dump_arrives_and_the_link_comes_up() {
    let fake = fake::Fake::start(0, 0).await;
    let mut switchers = Switchers::with_timing(fast());
    let atem = switchers.add("atem-main", &fake.addr.to_string()).unwrap();
    atem.wait_up(Duration::from_secs(5)).await.unwrap();
    let s = atem.snapshot();
    assert_eq!(s.link.sessions, 1);
    assert_eq!(s.state.product.as_deref(), Some("ATEM Mini Pro"));
    assert_eq!(s.state.protocol.map(|v| (v.major, v.minor)), Some((2, 30)));
    assert_eq!(s.state.inputs[&1].long_name, "Camera 1");
    assert_eq!(s.state.inputs[&4].short_name, "SLDS");
    assert_eq!((s.state.program(0), s.state.preview(0)), (Some(1), Some(2)));
    assert_eq!(s.state.mix_effects[0].keyers.len(), 1);
    assert_eq!(s.state.downstream_keyers[0].rate, 25);
    assert_eq!(s.state.aux, [10010]);
    assert_eq!(s.state.macros[&0].name, "Opening");
    assert_eq!(s.state.macros[&3].description, "");
    assert!(s.state.tally[&1].program && s.state.tally[&2].preview);
    fake.stop().await;
}

/// Every command, and the block the switcher must receive for it, written
/// out by hand.
fn every_command() -> Vec<(Command, Vec<u8>)> {
    let b = |name: &[u8; 4], body: &[u8]| {
        let mut out = vec![0, (8 + body.len()) as u8, 0, 0];
        out.extend(name);
        out.extend(body);
        out
    };
    let mut wipe = vec![0, 1, 0, 30];
    wipe.extend([0; 16]);
    vec![
        (Command::Program { me: 0, source: 3 }, b(b"CPgI", &[0, 0, 0, 3])),
        (Command::Preview { me: 0, source: 1000 }, b(b"CPvI", &[0, 0, 0x03, 0xe8])),
        (Command::Cut { me: 0 }, b(b"DCut", &[0, 0, 0, 0])),
        (Command::Auto { me: 0 }, b(b"DAut", &[0, 0, 0, 0])),
        (Command::TransitionPosition { me: 0, position: 2500 }, b(b"CTPs", &[0, 0, 0x09, 0xc4])),
        (Command::TransitionStyle { me: 0, style: TransitionStyle::Dip }, b(b"CTTp", &[1, 0, 1, 0])),
        (Command::TransitionRate { me: 0, style: RateStyle::Mix, frames: 50 }, b(b"CTMx", &[0, 50, 0, 0])),
        (Command::TransitionRate { me: 0, style: RateStyle::Dip, frames: 40 }, b(b"CTDp", &[1, 0, 40, 0, 0, 0, 0, 0])),
        (Command::TransitionRate { me: 0, style: RateStyle::Wipe, frames: 30 }, b(b"CTWp", &wipe)),
        (Command::DskOnAir { dsk: 0, on_air: true }, b(b"CDsL", &[0, 1, 0, 0])),
        // The fake reports protocol 2.30, so DDsA carries its mask byte.
        (Command::DskAuto { dsk: 0 }, b(b"DDsA", &[0, 0, 0, 0])),
        (Command::DskTie { dsk: 0, tie: true }, b(b"CDsT", &[0, 1, 0, 0])),
        (Command::DskRate { dsk: 0, frames: 15 }, b(b"CDsR", &[0, 15, 0, 0])),
        (Command::UskOnAir { me: 0, keyer: 0, on_air: true }, b(b"CKOn", &[0, 0, 1, 0])),
        (Command::FadeToBlack { me: 0 }, b(b"FtbA", &[0, 0, 0, 0])),
        (Command::FadeToBlackRate { me: 0, frames: 20 }, b(b"FtbC", &[1, 0, 20, 0])),
        (Command::AuxSource { aux: 0, source: 2 }, b(b"CAuS", &[1, 0, 0, 2])),
        (Command::MacroRun { index: 3 }, b(b"MAct", &[0, 3, 0, 0])),
        (Command::MacroStop, b(b"MAct", &[0xff, 0xff, 1, 0])),
        (Command::MacroStopRecording, b(b"MAct", &[0xff, 0xff, 2, 0])),
        (
            Command::MacroRecord { index: 5, name: "Hi".into(), description: String::new() },
            b(b"MSRc", &[0, 5, 0, 2, 0, 0, b'H', b'i']),
        ),
        (Command::MediaPlayerStill { player: 0, still: 4 }, b(b"MPSS", &[3, 0, 1, 4, 0, 0, 0, 0])),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn every_command_arrives_with_its_exact_bytes_in_order() {
    let fake = fake::Fake::start(0, 0).await;
    let mut switchers = Switchers::with_timing(fast());
    let atem = switchers.add("atem-main", &fake.addr.to_string()).unwrap();
    atem.wait_up(Duration::from_secs(5)).await.unwrap();
    let all = every_command();
    for (command, _) in &all {
        atem.send(vec![command.clone()]).await.unwrap();
    }
    let got = fake.wait_blocks(all.len()).await;
    for (i, (command, expected)) in all.iter().enumerate() {
        assert_eq!(&got[i], expected, "{command:?}");
    }
    {
        let seen = fake.seen.lock().unwrap();
        let ids: Vec<u16> = seen.headers.iter().map(|h| h.packet_id).collect();
        assert_eq!(ids, (1..=all.len() as u16).collect::<Vec<_>>(), "one packet each, numbered from 1");
        assert!(seen.headers.iter().all(|h| h.flags.has(Flags::ACK_REQUEST) && h.session & 0x8000 != 0));
    }
    fake.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_moves_the_state_the_switcher_reports() {
    let fake = fake::Fake::start(0, 0).await;
    let mut switchers = Switchers::with_timing(fast());
    let atem = switchers.add("atem-main", &fake.addr.to_string()).unwrap();
    atem.wait_up(Duration::from_secs(5)).await.unwrap();
    atem.send(vec![Command::Preview { me: 0, source: 3 }, Command::Cut { me: 0 }]).await.unwrap();
    let mut watch = atem.watch();
    let s = watch.wait_for(|s| s.state.program(0) == Some(3)).await.unwrap().clone();
    assert_eq!(s.state.preview(0), Some(1));
    assert!(s.state.tally[&3].program && s.state.tally[&1].preview);
    assert_eq!(fake.seen.lock().unwrap().headers.len(), 1, "two commands, one packet");
    fake.stop().await;
}
