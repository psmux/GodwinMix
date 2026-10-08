use super::*;

/// A WAV of a steady tone, made here with GStreamer so the test needs no
/// fixture. Ten seconds is longer than the test runs, so it never ends.
fn tone(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("tone.wav");
    let launch = format!(
        "audiotestsrc num-buffers=500 samplesperbuffer=960 volume=0.5 ! audio/x-raw,rate=48000,channels=2 ! wavenc ! filesink location=\"{}\"",
        path.display().to_string().replace('\\', "/")
    );
    let pipeline = gst::parse::launch(&launch).expect("the tone pipeline parses");
    pipeline.set_state(gst::State::Playing).unwrap();
    let bus = pipeline.bus().unwrap();
    let done = bus.timed_pop_filtered(gst::ClockTime::from_seconds(20), &[gst::MessageType::Eos, gst::MessageType::Error]);
    pipeline.set_state(gst::State::Null).unwrap();
    assert!(matches!(done.as_ref().map(|m| m.view()), Some(gst::MessageView::Eos(_))), "the tone was not written: {done:?}");
    path
}

/// A source with sound and no picture has a level of its own on the same
/// events as every other source.
///
/// The report: a USB microphone through the audio-device plugin was live and
/// no meter moved. The core was measuring it all along, because every
/// source's desk in the programme ends in a `level` whatever made the sound,
/// and the fault was in where the page drew it. This holds the core's half:
/// an input with no video says so in its status, says it has sound, and its
/// peaks arrive as `SourceAudioLevel` under its own id. A WAV file is that
/// input with nothing to install; the plugin's sound reaches the same branch.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_with_sound_and_no_picture_reports_its_own_level() {
    let _ = gst::init();
    if !crate::probe::exists("wavenc") {
        eprintln!("skipping: no wavenc here");
        return;
    }
    let dir = crate::observe::tempdir("sound-only");
    let uri = crate::input::file_uri(&tone(&dir));
    let cfg = programme_config(crate::config::Accel::Software);
    let (mut mix, _handle, mut cmds, mut bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    let mut events = mix.events.subscribe();
    mix.add_source(&SourceConfig::bare("mic", &uri), None).expect("adding the tone");

    let until = Instant::now() + Duration::from_secs(15);
    let mut loudest: Option<f64> = None;
    while Instant::now() < until && loudest.is_none_or(|db| db < -20.0) {
        while let Ok(ev) = bus.try_recv() {
            let _ = mix.handle(Command::Bus(ev));
        }
        while let Ok(cmd) = cmds.try_recv() {
            let _ = mix.handle(cmd);
        }
        while let Ok(envelope) = events.try_recv() {
            if let Event::SourceAudioLevel { source, peak_db } = envelope.event {
                if source.as_str() == "mic" {
                    let peak = peak_db.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    loudest = Some(loudest.map_or(peak, |l| l.max(peak)));
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let status = mix.status();
    let mic = status.sources.iter().find(|s| s.id.as_str() == "mic").expect("the source is listed");
    assert!(!mic.has_video, "a source with no picture said it had one, so no client can tell it apart");
    assert!(mic.has_audio, "a source making a sound said it had none, so no client draws its level");
    let loudest = loudest.expect("no level for a source with sound and no picture in 15 s");
    // A sine at half scale peaks at about -6 dBFS.
    assert!(loudest > -20.0, "the level does not reflect the tone: {loudest:.1} dBFS");
    mix.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
