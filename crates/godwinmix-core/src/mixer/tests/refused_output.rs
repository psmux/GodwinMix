use super::*;
use godwinmix_protocol::output_error::OutputErrorReason;

/// A port on 127.0.0.1 with nothing listening on it, so a connection is
/// refused at once rather than timing out.
fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// A server that takes the connection and hangs up straight away, which is
/// what a platform does to a stream key it does not know before the RTMP
/// handshake has even finished.
fn hanging_up_server() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            drop(conn);
        }
    });
    port
}

/// The first alert about `id`, within `within`.
async fn alert_about(
    events: &mut tokio::sync::broadcast::Receiver<crate::state::Envelope>,
    id: &str,
    within: Duration,
) -> Option<(Severity, String)> {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, events.recv()).await {
            Ok(Ok(env)) => {
                if let Event::Alert { severity, message, .. } = env.event {
                    if message.starts_with(id) {
                        return Some((severity, message));
                    }
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            _ => return None,
        }
    }
    None
}

/// A destination that never gets through says why, in its status and once
/// as an alert, and the programme does not miss a frame over it. Before
/// this, the reason went to a debug line and the page said "Reconnecting".
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_destination_says_why_and_the_programme_carries_on() {
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, mut bus) =
        Mixer::build(programme_config(crate::config::Accel::Software)).expect("mixer builds");
    mix.start().expect("the programme starts");
    let cfg: SourceConfig = toml::from_str("id = \"bars\"\nuri = \"test://smpte\"\n").unwrap();
    mix.add_source(&cfg, None).unwrap();
    let refused = format!("rtmp://127.0.0.1:{}/live/secret-key-1234", closed_port());
    let hung = format!("rtmp://127.0.0.1:{}/live/secret-key-1234", hanging_up_server());
    mix.add_output(&OutputConfig::bare("refused-out", &refused)).expect("it attaches");
    mix.add_output(&OutputConfig::bare("hung-out", &hung)).expect("it attaches");
    let frames = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counted = frames.clone();
    let vmix = mix.program.by_name("vmix").expect("the programme compositor");
    vmix.static_pad("src").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    let mut events = handle.subscribe();
    // Bus messages reach the mixer the way the binary sends them, which is
    // the whole path under test: the sink's error, read for its reason.
    let forward = handle.clone();
    tokio::spawn(async move {
        while let Some(ev) = bus.recv().await {
            if forward.send(Command::Bus(ev)).is_err() {
                return;
            }
        }
    });
    let thread = spawn(mix, cmd_rx, handle.clone());

    let within = Duration::from_secs(20).mul_f64(crate::plugin::harness::timing_slack());
    let (severity, message) = alert_about(&mut events, "refused-out", within).await.expect("an alert for the refused output");
    assert_eq!(severity, Severity::Error);
    assert!(message.contains("did not start") && message.contains("refused the connection"), "{message}");
    assert!(!message.contains("secret-key"), "the key reached an alert: {message}");

    let before = frames.load(Ordering::Relaxed);
    let status = handle.status().await.expect("status answers");
    let out = status.outputs.iter().find(|o| o.id == "refused-out").expect("the output is listed");
    let error = out.error.as_ref().expect("the status carries the reason");
    assert_eq!(error.reason, OutputErrorReason::Refused);
    assert_ne!(out.state, OutputState::Live);
    let wire = serde_json::to_string(&status.outputs).unwrap();
    assert!(!wire.contains("secret-key"), "the key reached the status: {wire}");

    // The hung up one: its reason is its own, not the other's. A dying
    // connection posts the sink's error and "Internal data stream error" from
    // upstream, in either order across a bus, and a specific reason replaces
    // a generic one when it lands (`Failure::note`), so a generic reason is
    // not yet the answer.
    let deadline = Instant::now() + within;
    let error = loop {
        let s = handle.status().await.expect("status answers");
        let e = s.outputs.iter().find(|o| o.id == "hung-out").and_then(|o| o.error.clone());
        let settled = e.as_ref().is_some_and(|e| e.reason != OutputErrorReason::Other);
        if settled || Instant::now() > deadline {
            break e;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let error = error.expect("the hung up output carries a reason");
    assert_eq!(error.reason, OutputErrorReason::Closed, "read from {:?}", error.detail);

    // Told once: the retries that follow do not raise it again.
    let again = alert_about(&mut events, "refused-out", Duration::from_secs(4)).await;
    assert!(again.is_none(), "a second alert for the same failure: {again:?}");
    let produced = frames.load(Ordering::Relaxed) - before;
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert!(produced > 30, "the programme produced {produced} frames in four seconds of failures");
}
