//! Audio buses and what they feed: an output, a sound card, a restart, and
//! the calls that are refused. The rig is in `buses.rs`.

use super::*;
use super::buses::{rig, sends};
use crate::buses::{AudioCall, Refusal};
use godwinmix_protocol::buses::{AudioStatus, BusConfig, BusSetRequest, MasterSetRequest, SendSetRequest};


#[tokio::test(flavor = "multi_thread")]
async fn the_desk_survives_a_restart() {
    let dir = crate::observe::tempdir("bus-store");
    let store = dir.join("godwinmix.runtime.toml");
    let mut rig = rig(&["cam1"]).await;
    rig.mix.persist_runtime_to(store.clone());
    rig.call(AudioCall::Master(MasterSetRequest { gain: Some(0.7), muted: Some(true) })).unwrap();
    rig.add(sends("monitor", &[("cam1", 0.25)]));
    rig.call(AudioCall::Set(BusSetRequest { id: "monitor".into(), gain: Some(0.5), ..Default::default() })).unwrap();
    let master = rig.mix.program.by_name("pgm-master").unwrap();
    assert!((master.property::<f64>("volume") - 0.7).abs() < 1e-6 && master.property::<bool>("mute"));
    rig.mix.shutdown();

    std::fs::write(dir.join("godwinmix.toml"), "").unwrap();
    let next = crate::config::Config::load(&dir.join("godwinmix.toml")).expect("the config loads");
    assert_eq!(next.audio.master.gain, 0.7);
    assert!(next.audio.master.muted);
    let bus = &next.audio.buses[0];
    assert_eq!((bus.id.as_str(), bus.gain, bus.sends["cam1"]), ("monitor", 0.5, 0.25));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_ids_and_values_say_what_to_do() {
    let mut rig = rig(&["cam1"]).await;
    let refused = |r: Result<AudioStatus, Refusal>| r.expect_err("this should be refused");
    let e = refused(rig.call(AudioCall::Add(BusConfig::named("Monitor A"))));
    assert!(matches!(&e, Refusal::Invalid { field: "id", .. }), "{e}");
    let e = refused(rig.call(AudioCall::Add(sends("monitor", &[("cam9", 1.0)]))));
    assert!(matches!(&e, Refusal::NotFound { kind: "source", known, .. } if known == &["cam1"]), "{e}");
    rig.add(sends("monitor", &[]));
    let e = refused(rig.call(AudioCall::Send(SendSetRequest { bus: "monitor".into(), source: "cam1".into(), level: 12.0 })));
    assert!(e.to_string().contains("0.0") && matches!(e, Refusal::Invalid { field: "level", .. }));
    let e = refused(rig.call(AudioCall::Send(SendSetRequest { bus: "nope".into(), source: "cam1".into(), level: 1.0 })));
    assert!(e.to_string().contains("audio.bus.add") && e.to_string().contains("monitor"), "{e}");
    let e = refused(rig.call(AudioCall::Master(MasterSetRequest { gain: Some(-1.0), muted: None })));
    assert!(matches!(e, Refusal::Invalid { field: "gain", .. }));

    let mut out = OutputConfig::bare("guest-out", "rtmp://127.0.0.1:1/live/x");
    out.audio = Some("monitor".into());
    out.enabled = false;
    rig.mix.add_output(&out).expect("a stopped output is kept");
    let e = refused(rig.call(AudioCall::Remove("monitor".into())));
    assert!(matches!(&e, Refusal::InUse { by, .. } if by == &["guest-out"]), "{e}");
    assert!(e.to_string().contains("output.set"), "{e}");
    rig.mix.shutdown();
}

/// The last buffer time seen on a pad, for comparing two timelines.
fn last_pts(pad: &gst::Pad) -> Arc<AtomicU64> {
    let seen = Arc::new(AtomicU64::new(0));
    let s = seen.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        if let Some(gst::PadProbeData::Buffer(b)) = &info.data {
            if let Some(pts) = b.pts() {
                s.store(pts.nseconds(), Ordering::Relaxed);
            }
        }
        gst::PadProbeReturn::Ok
    });
    seen
}

#[tokio::test(flavor = "multi_thread")]
async fn an_output_on_a_bus_is_fed_on_the_programme_timeline() {
    let dir = crate::observe::tempdir("bus-output");
    let mut rig = rig(&["cam1"]).await;
    // Well into the show, so a bus that started its timeline at zero would be
    // seconds behind the programme.
    tokio::time::sleep(Duration::from_secs(2)).await;
    rig.add(sends("recording-b", &[("cam1", 1.0)]));
    let programme = last_pts(&rig.mix.araw_tee.static_pad("sink").unwrap());
    let bus = last_pts(&rig.mix.program.by_name("bus-recording-b-tee").unwrap().static_pad("sink").unwrap());
    let mut out = OutputConfig::bare("iso", "record://programme");
    out.type_id = Some("record/output".into());
    out.params.insert("directory".into(), dir.to_string_lossy().to_string().into());
    out.audio = Some("recording-b".into());
    rig.mix.add_output(&out).expect("an output on a bus attaches");
    let _ = rig.peaks(1500).await;
    let (p, b) = (programme.load(Ordering::Relaxed) as i64, bus.load(Ordering::Relaxed) as i64);
    assert!(p > 0 && b > 0, "nothing flowed: programme {p}, bus {b}");
    assert!((p - b).abs() < 200_000_000, "the bus is {} ms off the programme", (b - p) / 1_000_000);
    assert!(rig.mix.program.by_name("bus-recording-b-enc-tee").is_some(), "no encoder for the output");
    let e = rig.call(AudioCall::Remove("recording-b".into())).expect_err("the bus is in use");
    assert!(matches!(e, Refusal::InUse { .. }));
    rig.mix.remove_output(&"iso".to_string()).expect("the output goes");
    assert!(rig.mix.program.by_name("bus-recording-b-enc-tee").is_none(), "the encoder outlived its last output");
    rig.mix.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A bus on a sound card plays in a pipeline of its own. A `fakesink` stands
/// in for the card, handed over the way a device search hands one back.
#[tokio::test(flavor = "multi_thread")]
async fn a_bus_on_a_sound_card_plays_beside_the_programme() {
    let mut rig = rig(&["cam1"]).await;
    rig.add(sends("phones", &[("cam1", 1.0)]));
    let sink = gst::ElementFactory::make("fakesink").build().unwrap();
    let played = Arc::new(Gaps::default());
    played.watch(&sink.static_pad("sink").unwrap());
    let found = crate::buses::DeviceFound { bus: "phones".into(), ticket: 0, sink: Ok(sink) };
    rig.call(AudioCall::DeviceFound(found)).expect("the card is taken");
    played.wait_for(50).await;
    let status = rig.call(AudioCall::List).unwrap();
    assert_eq!(status.buses[0].device_state, None, "a card that plays reported a fault");
    rig.call(AudioCall::Remove("phones".into())).expect("the bus goes with its card");
    assert!(rig.mix.program.by_name("bus-phones-dev-q").is_none(), "the card's feed outlived the bus");
    let named = crate::buses::device::sink_for("no such card anywhere").expect_err("a card that is not here");
    assert!(named.contains("audio.device.list"), "{named}");
    rig.mix.shutdown();
}
