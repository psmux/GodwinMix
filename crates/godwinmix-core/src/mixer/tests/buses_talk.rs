//! Talkback into a guest's return, with real elements: the operator's
//! source reaches the return only while it is held, and never the programme.

use super::buses::rig;
use crate::buses::{AudioCall, TalkCall};
use godwinmix_protocol::buses::BusConfig;

fn hold(on: bool) -> AudioCall {
    AudioCall::Talk(TalkCall { source: "talkback".into(), buses: vec!["guest-ana".into()], on })
}

#[tokio::test(flavor = "multi_thread")]
async fn talkback_reaches_the_guest_return_while_held_and_never_the_programme() {
    let mut rig = rig(&["host", "guest-ana", "talkback"]).await;
    rig.mix.take(Some("host".into()), None).expect("take the host");
    rig.mix.set_audio(&"host".to_string(), None, Some(true), None, &[]);
    rig.call(AudioCall::Talker { source: "talkback".into(), on: true }).expect("talkback is marked");
    let mut ret = BusConfig::named("guest-ana");
    ret.minus = Some("guest-ana".into());
    rig.call(AudioCall::AddGuest(ret)).expect("the guest's return is built");
    assert!(rig.mix.desk.saved().buses.is_empty(), "a guest's return is never saved");

    let _ = rig.peaks(600).await;
    let quiet = rig.peaks(1000).await;
    assert!(quiet["bus-guest-ana-level"] < -80.0, "talkback reached the return before it was held");

    rig.call(hold(true)).expect("talkback is held");
    let _ = rig.peaks(400).await;
    let held = rig.peaks(1200).await;
    let voice = held[&rig.meter_of("talkback")];
    assert!((voice - held["bus-guest-ana-level"]).abs() < 0.5, "held talkback is not in the return at its own level");
    assert!(held["pgm-level"] < -80.0, "talkback reached the programme: {:.1} dBFS", held["pgm-level"]);

    rig.mix.set_audio(&"talkback".to_string(), None, Some(false), None, &[]);
    let _ = rig.peaks(400).await;
    let unmuted = rig.peaks(1000).await;
    assert!(unmuted["pgm-level"] < -80.0, "unmuting a talkback source put it on air");

    rig.call(hold(false)).expect("talkback is let go");
    let _ = rig.peaks(400).await;
    let after = rig.peaks(1000).await;
    assert!(after["bus-guest-ana-level"] < -80.0, "talkback stayed in the return after it was let go");
    rig.mix.shutdown();
}
