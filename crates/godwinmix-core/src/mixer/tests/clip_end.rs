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

/// Frames reaching the clip's compositor pad, and the gaps in when they are
/// drawn: the longest step forward between two frames' running times, and
/// the longest step back, in milliseconds. Arrival times would say nothing:
/// the pad holds up to a second of frames waiting for their time, so a pass
/// that ends seamlessly still stops arriving a second before it stops being
/// drawn.
///
/// Measured where the branch's queue hands frames on, with the shift the
/// aligner gives every compositor pad drawing the source added: that sum is
/// when the frame is drawn, wherever it is read.
fn watch_pad(queue: &gst::Element, pads: Arc<crate::plugin::branch::VideoPads>) -> (Arc<AtomicU64>, Arc<AtomicU64>, Arc<AtomicU64>) {
    let (frames, gap, back) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    let (f, g, b) = (frames.clone(), gap.clone(), back.clone());
    let last = parking_lot::Mutex::new(None::<i64>);
    let pad = queue.static_pad("src").unwrap();
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let Some(gst::PadProbeData::Buffer(buffer)) = &info.data else { return gst::PadProbeReturn::Ok };
        f.fetch_add(1, Ordering::Relaxed);
        let segment = pad.sticky_event::<gst::event::Segment>(0);
        let Some(at) = segment
            .and_then(|s| s.segment().downcast_ref::<gst::ClockTime>().and_then(|s| s.to_running_time(buffer.pts()?)))
            .map(|t| t.nseconds() as i64 + pads.offset())
        else {
            return gst::PadProbeReturn::Ok;
        };
        if let Some(before) = last.lock().replace(at) {
            let step = (at - before) / 1_000_000;
            g.fetch_max(step.max(0) as u64, Ordering::Relaxed);
            b.fetch_max((-step).max(0) as u64, Ordering::Relaxed);
        }
        gst::PadProbeReturn::Ok
    });
    (frames, gap, back)
}

struct Clip {
    mix: Mixer,
    cmds: mpsc::Receiver<Command>,
    bus: mpsc::Receiver<BusEvent>,
    frames: Arc<AtomicU64>,
    gap: Arc<AtomicU64>,
    back: Arc<AtomicU64>,
}

async fn with_clip(name: &str, params: &str) -> Option<Clip> {
    with_clip_cfg(name, params, programme_config(crate::config::Accel::Software)).await
}

async fn with_clip_cfg(name: &str, params: &str, cfg: crate::config::Config) -> Option<Clip> {
    let _ = gst::init();
    let Some(path) = write_clip(name) else {
        println!("skipping: could not write a test clip with jpegenc and avimux");
        return None;
    };
    let (mut mix, _handle, cmds, bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    let uri = crate::input::file_uri(&path);
    let src: SourceConfig = toml::from_str(&format!("id = \"clip\"\nuri = \"{uri}\"\n{params}")).unwrap();
    mix.add_source(&src, None).expect("the clip is added");
    let id: SourceId = "clip".into();
    mix.take(Some(id.clone()), None).unwrap();
    let slot = mix.sources.iter().find(|s| s.input.id == id).unwrap();
    let (frames, gap, back) = watch_pad(&slot.branch.vq, slot.branch.pads.clone());
    Some(Clip { mix, cmds, bus, frames, gap, back })
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
    play(&mut c.mix, &mut c.cmds, &mut c.bus, 3).await;
    let after = c.frames.load(Ordering::Relaxed);
    let row = clip_row(&c.mix);
    c.mix.shutdown();

    assert!(run.ended.len() >= 2, "a two second clip played for seven seconds ended {} times", run.ended.len());
    assert!(run.ended.iter().all(|a| a == "repeat"), "{:?}", run.ended);
    assert!(!run.restarted, "the clip was restarted at its end rather than seeked");
    assert_eq!(run.states_after_live, vec![SourceState::Live], "the clip left live while it repeated");
    assert!(after > before, "the clip's picture stopped reaching the programme after repeating");
    // A frame lasts 40 ms. The seek, the decode and the queues between the
    // last frame of one pass and the first of the next may add a few more,
    // and no frame of a new pass may be due before the last of the old one.
    let (gap, back) = (c.gap.load(Ordering::Relaxed), c.back.load(Ordering::Relaxed));    assert!(gap < 250, "a pass started {gap} ms after the last frame of the one before was due");
    assert!(back < 20, "a pass started {back} ms before the last frame of the one before was drawn");
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

/// A held clip sends nothing more, so a mosaic built after it came to rest
/// had no frame of it and drew its tile black, and the Studio preview drawn
/// off that tile with it. Building one shows the last frame again, quietly.
#[tokio::test(flavor = "multi_thread")]
async fn a_mosaic_built_after_a_clip_came_to_rest_still_gets_its_last_frame() {
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.multiview.enabled = true;
    cfg.multiview.width = 320;
    cfg.multiview.height = 180;
    cfg.multiview.fps = 8;
    cfg.multiview.linger_secs = 0;
    let Some(mut c) = with_clip_cfg("late", "", cfg).await else { return };
    let first = play(&mut c.mix, &mut c.cmds, &mut c.bus, 5).await;
    assert_eq!(first.ended, vec!["hold".to_string()], "the clip came to rest before the mosaic was built");

    let shape = crate::multiview::MultiviewShape { fps: 8, width: 320, height: 180 };
    c.mix
        .multiview_demand(DemandAt { demand: Demand::Build(shape), generation: c.mix.mv.generation() })
        .expect("a subscriber builds the mosaic");
    let after = play(&mut c.mix, &mut c.cmds, &mut c.bus, 3).await;
    let warm = c.mix.mv.warm();
    let row = clip_row(&c.mix);
    c.mix.shutdown();

    assert!(warm, "the held clip's tile never got a frame");
    assert!(after.ended.is_empty(), "showing the last frame again was said as a new end: {:?}", after.ended);
    assert_eq!(row.extra.get("ended"), Some(&serde_json::Value::Bool(true)), "{row:?}");
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
    let again = play(&mut c.mix, &mut c.cmds, &mut c.bus, 3).await;
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
