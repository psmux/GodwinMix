//! A bus encoder comes up and goes down on threads of its own
//! (`buses::encode`). Two things that follows from: a failure to come up
//! arrives after the `output.add` that asked for it has answered, and still
//! has to reach the page; and a guest who leaves while the encoder is opening
//! must not leave the mixer waiting for it to finish opening.

use super::buses::{rig, sends};
use super::buses_outlets::bus_encoder;
use super::slow_encoder_start::{hold_state_lock, OPENING};
use super::*;
use crate::state::OutputState;
use parking_lot::Mutex;
use std::sync::Arc;

/// The longest a call the mixer answers may take while an encoder opens.
const ANSWERED: Duration = Duration::from_millis(500);

fn record_on_bus(id: &str, dir: &std::path::Path, bus: &str) -> OutputConfig {
    let mut out = OutputConfig::bare(id, "record://programme");
    out.type_id = Some("record/output".into());
    out.params.insert("directory".into(), dir.to_string_lossy().to_string().into());
    out.audio = Some(bus.into());
    out
}

async fn until_gone(program: &gst::Pipeline, bus: &str, within: Duration) -> bool {
    let until = Instant::now() + within;
    while bus_encoder(program, bus).is_some() && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    bus_encoder(program, bus).is_none()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bus_that_cannot_be_encoded_takes_its_output_off_the_air_and_says_why() {
    let dir = crate::observe::tempdir("bus-enc-fails");
    let mut rig = rig(&["cam1"]).await;
    rig.add(sends("guest-b", &[("cam1", 1.0)]));
    // A queue2 spilling to a folder that is not there fails on its way to
    // PAUSED, the way an encoder that cannot open its session does.
    // It passes raw sound, so the recording is one that takes raw sound.
    let nowhere = dir.join("no-such-folder").join("spill-XXXXXX");
    rig.mix.desk.encode_with("queue2", &[("temp-template", nowhere.to_str().unwrap())]);
    let mut events = rig.mix.events.subscribe();
    let mut out = record_on_bus("iso", &dir, "guest-b");
    out.params.insert("format".into(), "mkv".into());
    rig.mix.add_output(&out).expect("the add answers before the encoder is up");

    let until = Instant::now() + Duration::from_secs(10).mul_f64(crate::plugin::harness::timing_slack());
    let failed = |mix: &Mixer| mix.status().outputs.into_iter().find(|o| o.id == "iso" && o.state == OutputState::Failed);
    while failed(&rig.mix).is_none() && Instant::now() < until {
        let _ = rig.peaks(100).await;
    }
    let status = failed(&rig.mix).expect("the output is listed as failed");
    let why = status.shed.unwrap_or_default();
    let mut alert = None;
    while let Ok(env) = events.try_recv() {
        if let Event::Alert { message, .. } = env.event {
            if message.starts_with("iso ") {
                alert = Some(message);
            }
        }
    }
    let gone = until_gone(&rig.mix.program, "guest-b", Duration::from_secs(5)).await;
    let attached = rig.mix.outputs.iter().any(|o| o.id() == "iso");
    rig.mix.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(why.contains("guest-b") && why.contains("tried again") && why.contains("without `audio`"), "{why}");
    let alert = alert.expect("an alert about the output");
    assert!(alert.contains("could not be encoded"), "{alert}");
    assert!(!attached, "the output stayed on the air with no sound");
    assert!(gone, "the encoder that failed stayed in the programme");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_guest_leaving_while_its_bus_encoder_opens_does_not_hold_the_mixer() {
    let dir = crate::observe::tempdir("bus-enc-leave");
    let mut rig = rig(&["cam1"]).await;
    rig.add(sends("guest-b", &[("cam1", 1.0)]));
    // Hold the encoder's state lock from the moment it is added, as a
    // hardware encoder opening its session holds its own state change.
    let seen: Arc<Mutex<Vec<gst::Element>>> = Default::default();
    let mine = seen.clone();
    let hook = rig.mix.program.connect_element_added(move |_, el| {
        let name = el.name();
        if !name.starts_with("bus-guest-b-enc") {
            return;
        }
        mine.lock().push(el.clone());
        if name.ends_with("-enc") {
            hold_state_lock(el.clone()).recv().expect("the encoder's state lock is held");
        }
    });
    rig.mix.add_output(&record_on_bus("iso", &dir, "guest-b")).expect("the output is added");
    rig.mix.program.disconnect(hook);
    // The thread bringing it up is on the encoder by now.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let asked = Instant::now();
    rig.mix.remove_output(&"iso".to_string()).expect("the output goes");
    let took = asked.elapsed();

    let within = OPENING + Duration::from_secs(10).mul_f64(crate::plugin::harness::timing_slack());
    let gone = until_gone(&rig.mix.program, "guest-b", within).await;
    let chain = seen.lock().clone();
    let left: Vec<String> = chain
        .iter()
        .filter(|el| el.current_state() != gst::State::Null || el.parent().is_some())
        .map(|el| format!("{} {:?}", el.name(), el.current_state()))
        .collect();
    rig.mix.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(chain.len() >= 5, "the chain was not seen being built: {}", chain.len());
    assert!(took < ANSWERED, "output.remove took {took:?} while the bus encoder opened");
    assert!(gone, "the encoder was never taken down once it had opened");
    assert!(left.is_empty(), "elements left up or in the programme: {left:?}");
}

/// An output from the config is built once the programme is playing, so it
/// runs on the programme's clock and base time from the start rather than
/// on its own until a rebuild.
#[tokio::test(flavor = "multi_thread")]
async fn an_output_from_the_config_is_on_the_programme_clock() {
    let _ = gst::init();
    let dir = crate::observe::tempdir("config-output-clock");
    let mut cfg = programme_config(crate::config::Accel::Software);
    let mut out = OutputConfig::bare("archive", "record://programme");
    out.type_id = Some("record/output".into());
    out.params.insert("directory".into(), dir.to_string_lossy().to_string().into());
    cfg.outputs.push(out);
    let (mut mix, _handle, _cmds, _bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    let programme = (mix.program.clock(), mix.program.base_time());
    let output = mix.outputs.iter().find(|o| o.id() == "archive").and_then(|o| o.current_pipeline());
    let theirs = output.as_ref().map(|p| (Some(p.pipeline_clock()), p.base_time()));
    mix.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(programme.0.is_some(), "the programme has no clock once started");
    let (clock, base) = theirs.expect("the configured output has a pipeline");
    assert_eq!(clock, programme.0, "the configured output is on a clock of its own");
    assert_eq!(base, programme.1, "the configured output has a base time of its own");
}
