use super::*;

const STUCK: Duration = Duration::from_secs(3);

/// A destination that takes the connection and never says a word, which is
/// what `rtmp2sink` met on port 1935 on 2026-10-01. The connections are held
/// open for the life of the test process.
fn silent_server() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for conn in listener.incoming().flatten() {
            held.push(conn);
        }
    });
    port
}

/// An output whose reconnect takes seconds, because its old pipeline cannot
/// reach NULL while a thread is parked inside it, costs that output alone:
/// status and takes answer inside 100 ms and the programme keeps producing,
/// with the destination never answering the whole time.
#[tokio::test(flavor = "multi_thread")]
async fn a_reconnect_that_blocks_does_not_hold_the_mixer() {
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, _bus) =
        Mixer::build(programme_config(crate::config::Accel::Software)).expect("mixer builds");
    mix.start().expect("the programme starts");
    for (id, pattern) in [("one", "ball"), ("two", "smpte")] {
        let cfg: SourceConfig =
            toml::from_str(&format!("id = \"{id}\"\nuri = \"test://{pattern}\"\n")).unwrap();
        mix.add_source(&cfg, None).unwrap();
    }
    let id = "stuck-reconnect";
    let uri = format!("rtmp://127.0.0.1:{}/live/key", silent_server());
    mix.add_output(&OutputConfig::bare(id, &uri)).expect("the output attaches");
    // The first buffer into the output's pipeline parks the thread that
    // carried it across the proxy, for `STUCK`, once.
    let out = crate::observe::introspect::pipeline(&format!("output-{id}")).unwrap();
    let vq = out.by_name(&format!("out-{id}-mux-vq-0")).expect("the output's video queue");
    let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
    let once = std::sync::Mutex::new(Some(parked_tx));
    vq.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        if let Some(tx) = once.lock().unwrap().take() {
            let _ = tx.send(());
            std::thread::sleep(STUCK);
        }
        gst::PadProbeReturn::Ok
    });
    let frames = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counted = frames.clone();
    let vmix = mix.program.by_name("vmix").expect("the programme compositor");
    vmix.static_pad("src").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    let thread = spawn(mix, cmd_rx, handle.clone());
    parked_rx.recv_timeout(Duration::from_secs(10)).expect("a buffer reached the output");

    handle.request(|ack| Command::ReconnectOutput(id.into(), Some(ack))).await.unwrap();
    let take = |id: &str| {
        let source = Some(SourceId::from(id));
        move |ack| Command::Take { source, at_running_time_ms: None, ack: Some(ack) }
    };
    let before = frames.load(Ordering::Relaxed);
    let started = Instant::now();
    let mut slowest = Duration::ZERO;
    let mut calls = 0;
    while started.elapsed() < STUCK - Duration::from_millis(500) {
        let asked = Instant::now();
        handle.status().await.expect("status answers during the reconnect");
        slowest = slowest.max(asked.elapsed());
        let asked = Instant::now();
        let next = if calls % 2 == 0 { "one" } else { "two" };
        handle.request(take(next)).await.expect("a take answers during the reconnect");
        slowest = slowest.max(asked.elapsed());
        calls += 1;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let produced = frames.load(Ordering::Relaxed) - before;
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();

    assert!(slowest < Duration::from_millis(100), "a call took {slowest:?} during the reconnect");
    assert!(calls >= 10, "only {calls} rounds of calls in the window");
    assert!(produced > 30, "the programme produced {produced} frames in {STUCK:?}");
}
