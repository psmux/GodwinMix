use super::*;

/// How long the parked thread stays parked. Longer than any answer the tests
/// accept, so a removal that waited for it cannot pass.
const STUCK: Duration = Duration::from_secs(4);

/// The longest a removal may take to answer while a thread under it is
/// parked.
const ANSWERED: Duration = Duration::from_millis(500);

/// Park the first buffer that crosses `pad` for `STUCK`, once, and say when.
fn park_once(pad: &gst::Pad) -> std::sync::mpsc::Receiver<()> {
    let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
    let once = std::sync::Mutex::new(Some(parked_tx));
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        if let Some(tx) = once.lock().unwrap().take() {
            let _ = tx.send(());
            std::thread::sleep(STUCK);
        }
        gst::PadProbeReturn::Ok
    });
    parked_rx
}

/// A mixer with one source and one output that dials a server that never
/// answers, not yet running, and its bus kept open.
fn with_an_output(id: &str) -> (Mixer, MixerHandle, mpsc::Receiver<Command>, mpsc::Receiver<BusEvent>) {
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, bus) =
        Mixer::build(programme_config(crate::config::Accel::Software)).expect("mixer builds");
    mix.start().expect("the programme starts");
    let cfg: SourceConfig = toml::from_str("id = \"bars\"\nuri = \"test://smpte\"\n").unwrap();
    mix.add_source(&cfg, None).unwrap();
    let uri = format!("rtmp://127.0.0.1:{}/live/key", super::slow_output::silent_server());
    mix.add_output(&OutputConfig::bare(id, &uri)).expect("the output attaches");
    (mix, handle, cmd_rx, bus)
}

/// Remove `id` and say how long the answer took, then check status answers.
async fn timed_remove(handle: &MixerHandle, id: &str) -> Duration {
    let asked = Instant::now();
    handle.request(|ack| Command::RemoveOutput(id.into(), Some(ack))).await.expect("the removal answers");
    let took = asked.elapsed();
    let asked = Instant::now();
    handle.status().await.expect("status answers after the removal");
    assert!(asked.elapsed() < ANSWERED, "status took {:?} after the removal", asked.elapsed());
    took
}

/// An output whose feed thread is parked downstream of the programme, the
/// way a recorder's is when its pipeline stops draining, is removed at once.
/// On 2026-10-09 `output.remove` of a Quick Sync recording joined that thread
/// on the mixer thread for more than seventy seconds and the command queue
/// filled behind it.
#[tokio::test(flavor = "multi_thread")]
async fn removing_an_output_whose_feed_is_stuck_does_not_hold_the_mixer() {
    let id = "stuck-feed";
    let (mix, handle, cmd_rx, _bus) = with_an_output(id);
    let proxy = mix.program.by_name(&format!("out-{id}-vproxy")).expect("the output's proxy sink");
    let parked = park_once(&proxy.static_pad("sink").unwrap());
    let thread = spawn(mix, cmd_rx, handle.clone());
    let first = Duration::from_secs(10).mul_f64(crate::plugin::harness::timing_slack());
    parked.recv_timeout(first).expect("a buffer reached the output's feed");

    let took = timed_remove(&handle, id).await;
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert!(took < ANSWERED, "output.remove took {took:?} with the feed's thread parked");
}

/// The last consumer leaving stops the programme encoder, and taking an
/// encoder to NULL joins its streaming thread. One whose thread is held, as
/// a hardware encoder's driver can hold it, costs the mixer nothing.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_an_encoder_that_is_stuck_does_not_hold_the_mixer() {
    let id = "last-consumer";
    let (mix, handle, cmd_rx, _bus) = with_an_output(id);
    let venc = mix.program.by_name("venc").expect("the programme's video encoder");
    let parked = park_once(&venc.static_pad("src").unwrap());
    let enc = mix.encoder_handle();
    let thread = spawn(mix, cmd_rx, handle.clone());
    let first = Duration::from_secs(10).mul_f64(crate::plugin::harness::timing_slack());
    parked.recv_timeout(first).expect("a frame came out of the encoder");

    let took = timed_remove(&handle, id).await;
    let stopped = !enc.is_running();
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert!(took < ANSWERED, "output.remove took {took:?} with the encoder's thread parked");
    assert!(stopped, "the encoder still counts as running with nothing reading it");
}
