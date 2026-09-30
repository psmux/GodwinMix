use super::*;

/// How long the source's teardown is made to take. Long against the 100 ms the
/// mixer has to answer in, short enough that the test is quick.
const STUCK: Duration = Duration::from_secs(3);

/// A streaming thread that will not let go: the next buffer out of the
/// source's test pattern parks in a probe for `STUCK`, holding the stream
/// lock that taking the pipeline to NULL has to wait for. Returns once the
/// thread is parked.
pub(super) fn park_the_source(input: &InputPipeline) {
    let src = input
        .pipeline
        .iterate_recurse()
        .into_iter()
        .flatten()
        .find(|e| e.factory().is_some_and(|f| f.name() == "videotestsrc"))
        .expect("a test source has a videotestsrc");
    let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
    let once = std::sync::Mutex::new(Some(parked_tx));
    src.static_pad("src").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        if let Some(tx) = once.lock().unwrap().take() {
            let _ = tx.send(());
            std::thread::sleep(STUCK);
        }
        gst::PadProbeReturn::Ok
    });
    parked_rx.recv_timeout(Duration::from_secs(5)).expect("the source's thread parked");
}

/// The bug of 2026-10-01: a stall restart of one source held the mixer's
/// command loop, and every call behind it waited. A source whose teardown
/// takes seconds must cost that source and nothing else: status and takes
/// answer inside 100 ms the whole time, and the programme keeps producing.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_that_blocks_does_not_hold_the_mixer() {
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, _bus) =
        Mixer::build(programme_config(crate::config::Accel::Software)).expect("mixer builds");
    mix.start().expect("the programme starts");
    for (id, pattern) in [("slow", "ball"), ("steady", "smpte")] {
        let cfg: SourceConfig =
            toml::from_str(&format!("id = \"{id}\"\nuri = \"test://{pattern}\"\n")).unwrap();
        mix.add_source(&cfg, None).unwrap();
    }
    for _ in 0..100 {
        if mix.sources.iter().all(|s| matches!(s.input.observed_state(), SourceState::Live)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let frames = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counted = frames.clone();
    let vmix = mix.program.by_name("vmix").expect("the programme compositor");
    vmix.static_pad("src").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    let slow = mix.sources.iter().find(|s| s.input.id == "slow").unwrap().input.clone();
    let thread = spawn(mix, cmd_rx, handle.clone());
    let take = |id: &str| {
        let source = Some(SourceId::from(id));
        move |ack| Command::Take { source, at_running_time_ms: None, ack: Some(ack) }
    };
    handle.request(take("slow")).await.unwrap();

    park_the_source(&slow);
    handle.send(Command::RestartSource("slow".into())).unwrap();
    let before = frames.load(Ordering::Relaxed);
    let started = Instant::now();
    let mut slowest = Duration::ZERO;
    let mut calls = 0;
    while started.elapsed() < STUCK - Duration::from_millis(500) {
        let asked = Instant::now();
        handle.status().await.expect("status answers during the restart");
        slowest = slowest.max(asked.elapsed());
        let next = if calls % 2 == 0 { "steady" } else { "slow" };
        let asked = Instant::now();
        handle.request(take(next)).await.expect("a take answers during the restart");
        slowest = slowest.max(asked.elapsed());
        calls += 1;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let produced = frames.load(Ordering::Relaxed) - before;
    let still_restarting = slow.restarting();

    // And it does finish, and says so: the source comes back live.
    let mut back = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let st = handle.status().await.unwrap();
        if !slow.restarting()
            && st.sources.iter().any(|s| s.id == "slow" && matches!(s.state, SourceState::Live))
        {
            back = true;
            break;
        }
    }
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();

    assert!(slowest < Duration::from_millis(100), "a call took {slowest:?} during the restart");
    assert!(still_restarting, "the restart was not held up at all, so this proved nothing");
    assert!(calls >= 10, "only {calls} rounds of calls in the window");
    assert!(produced > 30, "the programme produced {produced} frames in {STUCK:?}");
    assert!(back, "the source never came back after its restart");
}

/// The order guarantees that were free while all of this ran on one thread:
/// a source removed in the middle of its restart stays removed, and adding it
/// again waits for the old pipeline to stop rather than building beside it.
#[tokio::test(flavor = "multi_thread")]
async fn a_remove_during_a_restart_wins_and_an_add_waits_for_it() {
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, _bus) =
        Mixer::build(programme_config(crate::config::Accel::Software)).expect("mixer builds");
    mix.start().expect("the programme starts");
    let cfg: SourceConfig = toml::from_str("id = \"slow\"\nuri = \"test://ball\"\n").unwrap();
    mix.add_source(&cfg, None).unwrap();
    for _ in 0..100 {
        if mix.sources.iter().all(|s| matches!(s.input.observed_state(), SourceState::Live)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let old = mix.sources[0].input.clone();
    let thread = spawn(mix, cmd_rx, handle.clone());

    park_the_source(&old);
    handle.send(Command::RestartSource("slow".into())).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let asked = Instant::now();
    handle.request(|ack| Command::RemoveSource("slow".into(), Some(ack))).await.unwrap();
    let remove_took = asked.elapsed();
    let asked = Instant::now();
    handle.status().await.unwrap();
    let status_took = asked.elapsed();
    // Answered once the old pipeline has stopped, inside the caller's five
    // seconds.
    handle.request(|ack| Command::AddSource(Box::new(cfg), Some(ack))).await.unwrap();
    let old_state = old.pipeline.current_state();
    let mut live = false;
    for _ in 0..100 {
        let st = handle.status().await.unwrap();
        let slow: Vec<_> = st.sources.iter().filter(|s| s.id == "slow").collect();
        if slow.len() == 1 && matches!(slow[0].state, SourceState::Live) {
            live = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();

    // The remove waits on the programme side for one compositor frame
    // (`gstutil::FRAME_BARRIER`, 150 ms at most) before it unbinds a slot, and
    // a parked source can make it wait all of that. Bounded, and not the
    // restart's seconds.
    assert!(remove_took < Duration::from_millis(400), "the remove took {remove_took:?}");
    assert!(status_took < Duration::from_millis(100), "status took {status_took:?}");
    assert_eq!(old_state, gst::State::Null, "the removed source's restart brought it back up");
    assert!(live, "the source added again under the same id never went live");
}
