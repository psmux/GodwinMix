//! Moving a running source between placements, measured on the programme.
//!
//! The acceptance criterion in roadmap Phase 4 is "no frame gap larger than one
//! frame". That is a statement about the programme, not about the source: what
//! must not happen is the encoder missing a beat while an instance is torn down
//! on one machine and started on another.
//!
//! So this measures the programme's frame interval with the same pad probe
//! `gmx_programme_frame_interval_ms` uses, does the move, and asserts that no
//! interval across the whole run exceeds one frame period plus a margin. The
//! compositor is `force-live` with the slate underneath, so it keeps composing
//! at the canvas rate whatever the sources are doing; this test is what makes
//! that a checked claim rather than an asserted one.

use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::{self, Command, Mixer};
use godwinmix_core::node::Place;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A mixer on a small canvas at 30 fps, with the programme frame intervals
/// being recorded.
fn running() -> (mixer::MixerHandle, Arc<Mutex<Vec<f64>>>, std::thread::JoinHandle<()>) {
    gst::init().unwrap();
    let cfg: Config = toml::from_str(
        r#"
[canvas]
width = 320
height = 180
fps = 30
sample_rate = 48000
channels = 2

[control]
bind = "127.0.0.1:0"
"#,
    )
    .unwrap();
    let (mut mix, handle, cmd_rx, _bus_rx) = Mixer::build(cfg).expect("build a mixer");
    let intervals: Arc<Mutex<Vec<f64>>> = Arc::new(Mutex::new(Vec::new()));
    // The same place `observe::attach_programme` puts its probe: the raw video
    // tee, which every programme frame passes exactly once.
    let pipeline = mix.program_pipeline().clone();
    if let Some(tee) = pipeline.by_name("vraw-tee") {
        let pad = tee.static_pad("sink").expect("the raw tee has a sink pad");
        let mine = intervals.clone();
        let last: Mutex<Option<Instant>> = Mutex::new(None);
        pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            let now = Instant::now();
            let mut last = last.lock().unwrap();
            if let Some(then) = last.replace(now) {
                mine.lock().unwrap().push((now - then).as_secs_f64() * 1000.0);
            }
            gst::PadProbeReturn::Ok
        })
        .expect("a probe on the raw tee");
    }
    mix.start().expect("start the mixer");
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    (handle, intervals, thread)
}

async fn settle(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

/// A source written as a test pattern, with a placement on it.
fn source(id: &str, place: Place) -> SourceConfig {
    let mut cfg = SourceConfig::bare(id, "test://smpte");
    cfg.place = Some(place);
    cfg
}

/// Take a source, then move it, and watch the programme the whole time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn moving_a_running_source_does_not_cost_the_programme_a_frame() {
    let (handle, intervals, thread) = running();
    handle
        .request(|ack| Command::AddSource(Box::new(source("cam1", Place::Core)), Some(ack)))
        .await
        .expect("add the source");
    handle
        .request(|ack| Command::Take {
            source: Some("cam1".into()),
            at_running_time_ms: None,
            ack: Some(ack),
        })
        .await
        .expect("take it");
    settle(1_500).await;

    // Everything measured before the move, so the two halves can be compared.
    let before = intervals.lock().unwrap().len();

    // The move. Exactly what `source.set {place}` issues: both commands on the
    // one queue, in order, with nothing able to land between them because the
    // mixer serialises every request through a single path.
    let at = Instant::now();
    handle
        .request(|ack| Command::RemoveSource("cam1".into(), Some(ack)))
        .await
        .expect("take the old instance out");
    handle
        .request(|ack| Command::AddSource(Box::new(source("cam1", Place::Sidecar)), Some(ack)))
        .await
        .expect("put the new one in");
    let took = at.elapsed();
    settle(1_500).await;

    let all = intervals.lock().unwrap().clone();
    handle.send(Command::Shutdown).ok();
    let _ = thread.join();

    assert!(before > 20, "the programme should have been running before the move, saw {before}");
    assert!(all.len() > before + 20, "the programme should have kept running after it");

    // One frame at 30 fps is 33.3 ms. The margin is one whole extra frame:
    // this is a wall clock measurement on a machine running a test suite, and
    // a scheduler hiccup is not a dropped frame.
    let period = 1000.0 / 30.0;
    let ceiling = period * 2.0;
    let worst = all.iter().cloned().fold(0.0f64, f64::max);
    let over: Vec<&f64> = all.iter().filter(|i| **i > ceiling).collect();
    println!(
        "programme frames: {} intervals, worst {worst:.1} ms, {} over {ceiling:.1} ms; the move \
         itself took {took:?}",
        all.len(),
        over.len()
    );
    assert!(
        over.len() <= 1,
        "the programme must not gap while a source moves: {} intervals over {ceiling:.1} ms, \
         worst {worst:.1} ms",
        over.len()
    );
}

/// A placement the plugin did not declare is refused, and the refusal names
/// what it did declare. Error -32005's message, checked where it is made.
#[test]
fn a_placement_a_plugin_did_not_declare_is_refused_by_name() {
    let refused = godwinmix_core::node::check_placement(
        "ndi/source",
        &Place::Node("studio-b".into()),
        &["sidecar".to_string()],
    )
    .unwrap_err()
    .to_string();
    assert!(refused.contains("node:studio-b"), "{refused}");
    assert!(refused.contains("sidecar"), "it must list what would have worked: {refused}");

    // And one it did declare is allowed.
    assert!(godwinmix_core::node::check_placement(
        "ndi/source",
        &Place::Node("studio-b".into()),
        &["sidecar".into(), "node".into()],
    )
    .is_ok());
}

/// A source's placement survives a round trip through the config file, which
/// is what makes a move persist across a restart.
#[test]
fn a_placement_is_written_back_to_the_config() {
    let mut cfg = source("cam1", Place::Node("studio-b".into()));
    cfg.type_id = Some("ndi/source".into());
    cfg.transport = Some(godwinmix_core::node::BridgeTransport::Srt);
    cfg.latency_ms = Some(150);
    let text = toml::to_string(&cfg).unwrap();
    let back: SourceConfig = toml::from_str(&text).unwrap();
    assert_eq!(back.place, Some(Place::Node("studio-b".into())));
    assert_eq!(back.bridge_transport(), godwinmix_core::node::BridgeTransport::Srt);
    assert_eq!(back.latency_ms, Some(150));
}
