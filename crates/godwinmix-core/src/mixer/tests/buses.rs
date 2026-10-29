//! Audio buses on a running programme, with real elements: what is built and
//! when, what each kind of bus carries, and what the programme sees while a
//! bus comes and goes.

use super::*;
use crate::buses::{AudioCall, Refusal};
use godwinmix_protocol::buses::{AudioStatus, BusConfig, BusSetRequest, CueSetRequest, MasterSetRequest, SendSetRequest};

pub(super) struct Rig {
    pub mix: Mixer,
    pub bus: mpsc::Receiver<BusEvent>,
    pub cmds: mpsc::Receiver<Command>,
}

/// A programme with these test sources, live, keeping the bus messages so
/// the meters can be read.
pub(super) async fn rig(ids: &[&str]) -> Rig {
    let _ = gst::init();
    let cfg = programme_config(crate::config::Accel::Software);
    let (mut mix, _handle, cmds, bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    for id in ids {
        let cfg = SourceConfig::bare(id, "test://smpte");
        mix.add_source(&cfg, None).unwrap_or_else(|e| panic!("adding {id}: {e:#}"));
    }
    let rig = Rig { mix, bus, cmds };
    for _ in 0..100 {
        let live = |id: &&str| rig.mix.sources.iter().any(|s| s.input.id == *id && matches!(s.input.observed_state(), SourceState::Live));
        if ids.iter().all(live) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    rig
}

impl Rig {
    pub fn call(&mut self, call: AudioCall) -> Result<AudioStatus, Refusal> {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        self.mix.audio_call(call, Some(tx));
        rx.try_recv().expect("an audio call answers at once")
    }

    pub fn add(&mut self, cfg: BusConfig) {
        let id = cfg.id.clone();
        self.call(AudioCall::Add(cfg)).unwrap_or_else(|e| panic!("adding bus {id}: {e}"));
    }

    /// The loudest peak each meter reported over `ms`, by element name.
    pub async fn peaks(&mut self, ms: u64) -> HashMap<String, f64> {
        while self.bus.try_recv().is_ok() {}
        let mut out: HashMap<String, f64> = HashMap::new();
        let until = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < until {
            while let Ok(ev) = self.bus.try_recv() {
                if let BusEvent::Level { src, peak_db } = ev {
                    let p = peak_db.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    let e = out.entry(src).or_insert(f64::NEG_INFINITY);
                    *e = e.max(p);
                }
            }
            while let Ok(cmd) = self.cmds.try_recv() {
                let _ = self.mix.handle(cmd);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        out
    }

    pub fn meter_of(&self, source: &str) -> String {
        self.mix.sources.iter().find(|s| s.input.id == source).expect("the source is here").branch.meter.clone()
    }

    pub fn element_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.mix.program.iterate_recurse().into_iter().flatten().map(|e| e.name().to_string()).collect();
        names.sort();
        names
    }
}

fn sends(id: &str, to: &[(&str, f64)]) -> BusConfig {
    let mut cfg = BusConfig::named(id);
    cfg.sends = to.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    cfg
}

#[tokio::test(flavor = "multi_thread")]
async fn no_bus_means_the_programme_is_built_as_it_always_was() {
    let mut rig = rig(&["cam1"]).await;
    let before = rig.element_names();
    assert!(
        !before.iter().any(|n| n.starts_with("bus-") || n.starts_with("pgm-asend") || n.starts_with("pgm-ab")),
        "a programme with no bus has bus elements in it: {before:?}"
    );
    rig.add(sends("monitor", &[("cam1", 1.0)]));
    let during = rig.element_names();
    assert!(during.iter().any(|n| n == "bus-monitor-mix"), "the bus was not built: {during:?}");
    assert!(during.iter().any(|n| n.starts_with("pgm-asend-cam1")), "the source has no send point");
    rig.call(AudioCall::Remove("monitor".into())).expect("the bus goes");
    assert_eq!(rig.element_names(), before, "the last bus to go left something behind");
    rig.mix.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_reaches_its_bus_at_its_level() {
    let mut rig = rig(&["tone"]).await;
    rig.add(sends("monitor", &[("tone", 0.5)]));
    let _ = rig.peaks(600).await;
    let peaks = rig.peaks(1500).await;
    let source = peaks[&rig.meter_of("tone")];
    let bus = *peaks.get("bus-monitor-level").expect("the bus meter reported");
    // Half the amplitude is 6.02 dB down.
    assert!((source - bus - 6.02).abs() < 0.5, "source {source:.2} dBFS, bus {bus:.2} dBFS");

    rig.call(AudioCall::Send(SendSetRequest { bus: "monitor".into(), source: "tone".into(), level: 0.0 })).unwrap();
    let _ = rig.peaks(400).await;
    let off = rig.peaks(1000).await["bus-monitor-level"];
    assert!(off < -80.0, "a send at 0 still reaches the bus: {off:.1} dBFS");
    rig.mix.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mix_minus_is_the_programme_without_its_source() {
    let mut rig = rig(&["host", "guest-1"]).await;
    rig.mix.take(Some("host".into()), None).expect("take the host");
    let mut minus_host = BusConfig::named("minus-host");
    minus_host.minus = Some("host".into());
    let mut minus_guest = BusConfig::named("guest-return");
    minus_guest.minus = Some("guest-1".into());
    rig.add(minus_host);
    rig.add(minus_guest);
    let _ = rig.peaks(600).await;
    let peaks = rig.peaks(1500).await;
    let host = peaks[&rig.meter_of("host")];
    let without_guest = peaks["bus-guest-return-level"];
    let without_host = peaks["bus-minus-host-level"];
    assert!((host - without_guest).abs() < 0.5, "the guest's return lost the host: host {host:.1}, bus {without_guest:.1}");
    assert!(without_host < -80.0, "a mix minus still carries its own source: {without_host:.1} dBFS");
    rig.mix.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cue_bus_hears_a_marked_source_that_is_off_air_and_muted() {
    let mut rig = rig(&["cam1", "mic"]).await;
    rig.mix.take(Some("cam1".into()), None).expect("take cam1");
    rig.mix.set_audio(&"mic".to_string(), None, Some(true), None, &[]);
    let mut cue = BusConfig::named("phones");
    cue.cue = true;
    rig.add(cue);
    rig.call(AudioCall::Cue(CueSetRequest { source: "mic".into(), cue: true, solo: true })).unwrap();
    let _ = rig.peaks(600).await;
    let peaks = rig.peaks(1500).await;
    let mic = peaks[&rig.meter_of("mic")];
    assert!((mic - peaks["bus-phones-level"]).abs() < 0.5, "the cue bus does not carry the marked mic");
    let programme = peaks["pgm-level"];
    assert!((programme - peaks[&rig.meter_of("cam1")]).abs() < 0.5, "cue changed what is on air");
    rig.mix.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bus_added_and_removed_live_costs_the_programme_nothing() {
    let mut rig = rig(&["cam1"]).await;
    rig.mix.take(Some("cam1".into()), None).expect("take cam1");
    let out = Arc::new(Gaps::default());
    out.watch(&rig.mix.araw_tee.static_pad("sink").unwrap());
    let into = Arc::new(Gaps::default());
    let apad = rig.mix.sources[0].branch.apad.clone();
    into.watch(&apad);
    out.wait_for(50).await;
    for round in 0..3 {
        rig.add(sends("monitor", &[("cam1", 1.0)]));
        out.wait_for(20).await;
        rig.call(AudioCall::Remove("monitor".into())).unwrap_or_else(|e| panic!("round {round}: {e}"));
        out.wait_for(20).await;
    }
    into.wait_for(20).await;
    // Ten millisecond buffers: anything past two of them is sound that never
    // came.
    let (o, i) = (out.largest.load(Ordering::Relaxed), into.largest.load(Ordering::Relaxed));
    assert!(o <= 21_000_000, "the programme's sound had a gap of {} ms", o / 1_000_000);
    assert!(i <= 21_000_000, "the source's sound into the mixer had a gap of {} ms", i / 1_000_000);
    rig.mix.shutdown();
}

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
