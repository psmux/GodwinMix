//! A real clip played to its end through a real programme, each way.

use super::*;
use std::sync::atomic::AtomicU64;

/// A two second clip with picture and sound, written with elements every
/// GStreamer install has. AVI, so it takes the ordinary decode path rather
/// than the one a container that may hold alpha gets.
fn write_clip(name: &str) -> Option<std::path::PathBuf> {
    let dir = std::env::temp_dir().join(format!("gmx-clip-end-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("clip.avi");
    let desc = format!(
        "videotestsrc num-buffers=50 pattern=ball ! video/x-raw,width=320,height=180,framerate=25/1 ! jpegenc ! \
         avimux name=m ! filesink location=\"{}\" \
         audiotestsrc num-buffers=86 samplesperbuffer=1024 ! audio/x-raw,format=S16LE,rate=44100,channels=2 ! m.",
        path.display().to_string().replace('\\', "/")
    );
    let pipeline = gst::parse::launch(&desc).ok()?;
    pipeline.set_state(gst::State::Playing).ok()?;
    let done = pipeline.bus()?.timed_pop_filtered(
        gst::ClockTime::from_seconds(20),
        &[gst::MessageType::Eos, gst::MessageType::Error],
    );
    let _ = pipeline.set_state(gst::State::Null);
    done.filter(|m| m.type_() == gst::MessageType::Eos).map(|_| path)
}

/// What a run saw: every state the source passed through after it first went
/// live, whether it was restarted, and each `event/source.ended`.
#[derive(Default)]
struct Run {
    states_after_live: Vec<SourceState>,
    restarted: bool,
    ended: Vec<String>,
}

/// The mixer loop by hand, as `mixer::spawn` runs it, ticks included.
async fn play(mix: &mut Mixer, cmds: &mut mpsc::Receiver<Command>, bus: &mut mpsc::Receiver<BusEvent>, secs: u64) -> Run {
    let id: SourceId = "clip".into();
    let mut events = mix.events.subscribe();
    let mut run = Run::default();
    let mut live = false;
    let until = Instant::now() + Duration::from_secs(secs);
    let mut next_tick = Instant::now();
    while Instant::now() < until {
        while let Ok(ev) = bus.try_recv() {
            let _ = mix.handle(Command::Bus(ev));
        }
        while let Ok(cmd) = cmds.try_recv() {
            run.restarted |= matches!(cmd, Command::RetrySource(..) | Command::RestartSource(_));
            let _ = mix.handle(cmd);
        }
        if Instant::now() >= next_tick {
            mix.tick();
            next_tick = Instant::now() + TICK;
        }
        while let Ok(envelope) = events.try_recv() {
            if let Event::SourceEnded { source, at_end } = envelope.event {
                assert_eq!(source, "clip");
                run.ended.push(at_end);
            }
        }
        let state = mix.sources.iter().find(|s| s.input.id == id).map(|s| s.input.observed_state());
        live |= state == Some(SourceState::Live);
        if live {
            run.states_after_live.extend(state.filter(|s| run.states_after_live.last() != Some(s)));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    run
}

/// Frames reaching the clip's compositor pad, and the longest wait between
/// two of them, in milliseconds.
fn watch_pad(pad: &gst::Pad) -> (Arc<AtomicU64>, Arc<AtomicU64>) {
    let (frames, gap) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    let (f, g) = (frames.clone(), gap.clone());
    let last = parking_lot::Mutex::new(None::<Instant>);
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        let now = Instant::now();
        if let Some(before) = last.lock().replace(now) {
            g.fetch_max(now.duration_since(before).as_millis() as u64, Ordering::Relaxed);
        }
        f.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    (frames, gap)
}

struct Clip {
    mix: Mixer,
    cmds: mpsc::Receiver<Command>,
    bus: mpsc::Receiver<BusEvent>,
    frames: Arc<AtomicU64>,
    gap: Arc<AtomicU64>,
}

async fn with_clip(name: &str, params: &str) -> Option<Clip> {
    let _ = gst::init();
    let Some(path) = write_clip(name) else {
        println!("skipping: could not write a test clip with jpegenc and avimux");
        return None;
    };
    let cfg = programme_config(crate::config::Accel::Software);
    let (mut mix, _handle, cmds, bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    let uri = crate::input::file_uri(&path);
    let src: SourceConfig = toml::from_str(&format!("id = \"clip\"\nuri = \"{uri}\"\n{params}")).unwrap();
    mix.add_source(&src, None).expect("the clip is added");
    let id: SourceId = "clip".into();
    mix.take(Some(id.clone()), None).unwrap();
    let pad = mix.pool.slots().iter().find(|s| s.source() == Some(&id)).unwrap().pad().clone();
    let (frames, gap) = watch_pad(&pad);
    Some(Clip { mix, cmds, bus, frames, gap })
}

fn clip_row(mix: &Mixer) -> crate::state::SourceStatus {
    mix.status().sources.into_iter().find(|s| s.id == "clip").expect("the clip is in the status")
}

/// The 0.2.2 report: a clip at its end read `connecting` and started again
/// with its last second missing. Set to repeat, it now goes round by a seek,
/// stays live the whole time, and its pictures keep reaching the compositor
/// with no gap a person would see.
#[tokio::test(flavor = "multi_thread")]
async fn a_clip_set_to_repeat_goes_round_by_seeking_and_never_reads_connecting() {
    let Some(mut c) = with_clip("repeat", "params = { at_end = \"repeat\" }").await else { return };
    let run = play(&mut c.mix, &mut c.cmds, &mut c.bus, 7).await;
    let before = c.frames.load(Ordering::Relaxed);
    play(&mut c.mix, &mut c.cmds, &mut c.bus, 1).await;
    let after = c.frames.load(Ordering::Relaxed);
    let row = clip_row(&c.mix);
    c.mix.shutdown();

    assert!(run.ended.len() >= 2, "a two second clip played for seven seconds ended {} times", run.ended.len());
    assert!(run.ended.iter().all(|a| a == "repeat"), "{:?}", run.ended);
    assert!(!run.restarted, "the clip was restarted at its end rather than seeked");
    assert_eq!(run.states_after_live, vec![SourceState::Live], "the clip left live while it repeated");
    assert!(after > before, "the clip's picture stopped reaching the programme after repeating");
    let gap = c.gap.load(Ordering::Relaxed);
    assert!(gap < 400, "the longest wait between two of the clip's pictures was {gap} ms");
    assert_eq!(row.extra.get("at_end"), Some(&serde_json::json!("repeat")));
    assert_eq!(row.extra.get("ended"), None, "a repeating clip is never held");
}

/// A clip with nothing said holds its last frame, reads live with `ended`,
/// and is neither restarted nor judged stalled however long it is left.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_clip_holds_its_last_frame() {
    let Some(mut c) = with_clip("hold", "").await else { return };
    let run = play(&mut c.mix, &mut c.cmds, &mut c.bus, 6).await;
    let row = clip_row(&c.mix);
    c.mix.shutdown();

    assert_eq!(run.ended, vec!["hold".to_string()], "the clip should end once and stay ended");
    assert!(!run.restarted, "a held clip was restarted at its end");
    assert_eq!(run.states_after_live, vec![SourceState::Live], "the held clip left live");
    assert_eq!(row.extra.get("ended"), Some(&serde_json::Value::Bool(true)), "{row:?}");
    assert_eq!(row.extra.get("at_end"), Some(&serde_json::json!("hold")));
}

/// `leave` holds the clip the same way and says so on the event bus, which
/// is where the control plane hears it and makes the take. Then setting it
/// to repeat while it is held starts it again at once, with no restart.
#[tokio::test(flavor = "multi_thread")]
async fn a_clip_set_to_leave_says_so_and_repeat_set_while_held_plays_it_again() {
    let Some(mut c) = with_clip("leave", "params = { at_end = \"leave\" }").await else { return };
    let run = play(&mut c.mix, &mut c.cmds, &mut c.bus, 5).await;
    assert_eq!(run.ended, vec!["leave".to_string()]);
    assert_eq!(clip_row(&c.mix).extra.get("ended"), Some(&serde_json::Value::Bool(true)));

    let repeat: crate::config::Params = toml::from_str("at_end = \"repeat\"").unwrap();
    let id: SourceId = "clip".into();
    let applied = c.mix.sources.iter().find(|s| s.input.id == id).unwrap().input.configure(&repeat).unwrap();
    assert!(matches!(applied, crate::plugin::Configure::Applied), "at_end alone is taken in place");
    c.mix.clip_reconfigured(&id);
    let before = c.frames.load(Ordering::Relaxed);
    let again = play(&mut c.mix, &mut c.cmds, &mut c.bus, 1).await;
    let after = c.frames.load(Ordering::Relaxed);
    let row = clip_row(&c.mix);
    c.mix.shutdown();

    assert!(!run.restarted && !again.restarted, "the clip was restarted");
    assert!(after > before + 10, "the clip did not play again: {before} then {after} frames");
    assert_eq!(row.extra.get("ended"), None, "{row:?}");
}

/// A client following `event/source.state` hears `live` when a source starts
/// delivering, rather than being left at the `connecting` it was told on add.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_that_starts_delivering_is_said_to_be_live() {
    let _ = gst::init();
    let cfg = programme_config(crate::config::Accel::Software);
    let (mut mix, _handle, mut cmds, mut bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    let mut events = mix.events.subscribe();
    let src: SourceConfig = toml::from_str("id = \"cam\"\nuri = \"test://ball\"\n").unwrap();
    mix.add_source(&src, None).unwrap();
    let mut said = Vec::new();
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until && !said.contains(&SourceState::Live) {
        while let Ok(ev) = bus.try_recv() {
            let _ = mix.handle(Command::Bus(ev));
        }
        while let Ok(cmd) = cmds.try_recv() {
            let _ = mix.handle(cmd);
        }
        mix.tick();
        while let Ok(envelope) = events.try_recv() {
            if let Event::SourceStateChanged { source, state } = envelope.event {
                if source == "cam" {
                    said.push(state);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    mix.shutdown();
    assert_eq!(said, vec![SourceState::Connecting, SourceState::Live]);
}
