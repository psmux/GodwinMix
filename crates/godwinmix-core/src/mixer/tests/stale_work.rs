use super::*;

/// Hand the mixer what its own workers and timers sent, as its thread would,
/// for `how_long`.
async fn pump(mix: &mut Mixer, rx: &mut mpsc::Receiver<Command>, how_long: Duration) {
    let until = Instant::now() + how_long;
    while Instant::now() < until {
        while let Ok(cmd) = rx.try_recv() {
            let _ = mix.handle(cmd);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn hall(mix: &Mixer) -> Option<&SourceSlot> {
    mix.sources.iter().find(|s| s.input.id == "hall")
}

async fn until_live(mix: &mut Mixer, rx: &mut mpsc::Receiver<Command>) -> bool {
    for _ in 0..200 {
        if hall(mix).is_some_and(|s| matches!(s.input.observed_state(), SourceState::Live)) {
            return true;
        }
        pump(mix, rx, Duration::from_millis(25)).await;
    }
    false
}

/// Every stream start out of a source's test pattern. A restart takes the
/// pipeline to NULL and back, and the pattern starts its stream again.
fn count_stream_starts(input: &InputPipeline) -> Arc<AtomicU64> {
    let src = input
        .pipeline
        .iterate_recurse()
        .into_iter()
        .flatten()
        .find(|e| e.factory().is_some_and(|f| f.name() == "videotestsrc"))
        .expect("a test source has a videotestsrc");
    let starts = Arc::new(AtomicU64::new(0));
    let seen = starts.clone();
    let pad = src.static_pad("src").unwrap();
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        if info.event().is_some_and(|e| e.type_() == gst::EventType::StreamStart) {
            seen.fetch_add(1, Ordering::Relaxed);
        }
        gst::PadProbeReturn::Ok
    });
    starts
}

/// The replay of `tests/sessions/source-stall.jsonl` after restarts moved off
/// the mixer thread: `hall` was removed with a retry armed, added again under
/// the same id, taken, and the old one's retry restarted the new one on air.
/// Work started for one instance of an id must leave the next one alone, here
/// with a restart of the old one stuck in its teardown as well.
#[tokio::test(flavor = "multi_thread")]
async fn work_for_a_removed_source_never_touches_the_one_added_in_its_place() {
    let _ = gst::init();
    let (mut mix, _handle, mut rx, _bus) =
        Mixer::build(programme_config(crate::config::Accel::Software)).expect("mixer builds");
    mix.start().expect("the programme starts");
    let cfg: SourceConfig = toml::from_str("id = \"hall\"\nuri = \"test://ball\"\n").unwrap();
    mix.add_source(&cfg, None).unwrap();
    assert!(until_live(&mut mix, &mut rx).await, "the first hall never went live");
    let id: SourceId = "hall".into();
    let old = hall(&mix).unwrap().input.clone();
    let old_generation = hall(&mix).unwrap().generation;

    // A retry the supervisor armed for the old hall after four failures, due
    // in a little over five seconds: after the new hall is in.
    mix.source_attempts.insert(id.clone(), 4);
    mix.arm_source_restart(id.clone(), "the test armed it");
    // And a restart of the old hall stuck in its teardown for three.
    super::slow_restart::park_the_source(&old);
    mix.handle(Command::RestartSource(id.clone())).unwrap();
    pump(&mut mix, &mut rx, Duration::from_millis(100)).await;
    assert!(old.restarting(), "the old hall's restart is not running, so this proves nothing");

    mix.handle(Command::RemoveSource(id.clone(), None)).unwrap();
    mix.handle(Command::AddSource(Box::new(cfg), None)).unwrap();
    assert!(until_live(&mut mix, &mut rx).await, "the new hall never went live");
    let new = hall(&mix).unwrap().input.clone();
    let new_generation = hall(&mix).unwrap().generation;
    let starts = count_stream_starts(&new);
    mix.take(Some(id.clone()), None).unwrap();

    // Past the retry's due time and the old restart's end.
    pump(&mut mix, &mut rx, Duration::from_secs(4)).await;
    let now = hall(&mix)
        .map(|s| (s.generation, Arc::ptr_eq(&s.input, &new), s.input.observed_state()));
    let old_state = old.pipeline.current_state();
    mix.shutdown();

    assert_ne!(old_generation, new_generation, "the new hall has the old one's generation");
    assert_eq!(old_state, gst::State::Null, "the old hall came back up");
    assert_eq!(
        now,
        Some((new_generation, true, SourceState::Live)),
        "the new hall was replaced or is not live"
    );
    assert_eq!(starts.load(Ordering::Relaxed), 0, "the new hall was restarted for the old one");
}

/// The same for an output: a reconnect armed for one output is dropped once
/// that output has been removed, and the one added under its id is left to
/// connect in peace.
#[tokio::test(flavor = "multi_thread")]
async fn a_reconnect_armed_for_a_removed_output_leaves_its_successor_alone() {
    let _ = gst::init();
    let (mut mix, _handle, mut rx, _bus) =
        Mixer::build(programme_config(crate::config::Accel::Software)).expect("mixer builds");
    mix.start().expect("the programme starts");
    let uri = format!("rtmp://127.0.0.1:{}/live/key", super::slow_output::silent_server());
    let cfg = OutputConfig::bare("dest", &uri);
    mix.add_output(&cfg).expect("the output attaches");
    let delay = mix.outputs[0].cfg.reconnect_policy().delay_for(0);
    mix.arm_output_reconnect("dest".into());

    mix.handle(Command::RemoveOutput("dest".into(), None)).unwrap();
    mix.add_output(&cfg).expect("the output attaches again");
    let new = mix.outputs[0].clone();
    pump(&mut mix, &mut rx, delay + Duration::from_secs(1)).await;
    let still = mix.outputs.iter().any(|o| Arc::ptr_eq(o, &new));
    let reconnects = new.status().reconnects;
    mix.shutdown();

    assert!(still, "the new output was replaced");
    assert_eq!(reconnects, 0, "the new output was reconnected for the old one");
}
