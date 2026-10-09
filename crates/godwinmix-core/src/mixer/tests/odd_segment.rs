use super::full_pool::{count_pad, count_programme};
use super::*;

/// Run the mixer loop by hand for `secs`: bus events and commands, as
/// `mixer::spawn` would. Answers whether a source restart came through.
async fn run_loop(
    mix: &mut Mixer,
    cmds: &mut mpsc::Receiver<Command>,
    bus: &mut mpsc::Receiver<BusEvent>,
    secs: u64,
) -> bool {
    let mut restarted = false;
    let until = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < until {
        while let Ok(ev) = bus.try_recv() {
            let _ = mix.handle(Command::Bus(ev));
        }
        while let Ok(cmd) = cmds.try_recv() {
            restarted |= matches!(cmd, Command::RetrySource(..));
            let _ = mix.handle(cmd);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    restarted
}

/// A source whose own pipeline sends a byte segment towards the programme.
///
/// The compositor places a frame by its running time. Its own pads refuse a
/// segment that is not in time and keep the one they had, so in this test,
/// without the guard, nothing aborted: the source went on with its frames
/// placed by a segment it had replaced, and nothing restarted it. Everything
/// else on the way reads that segment too (the crop and flip of a slot, the
/// mosaic's tiles, the timeline aligner), and a frame with no running time is
/// a `g_assert` in `gst_video_aggregator_fill_queues` that takes the process
/// down. So the guard on the source's proxy sink holds the segment and the
/// frames behind it back, the source's pipeline posts an error, and the mixer
/// restarts that source. The programme does not miss a beat and the source
/// comes back.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_sending_a_byte_segment_is_restarted_and_the_programme_runs_on() {
    let _ = gst::init();
    let cfg = programme_config(crate::config::Accel::Software);
    let (mut mix, _handle, mut cmds, mut bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    for (id, pattern) in [("odd", "smpte"), ("steady", "ball")] {
        let cfg: SourceConfig = toml::from_str(&format!("id = \"{id}\"\nuri = \"test://{pattern}\"\n")).unwrap();
        mix.add_source(&cfg, None).unwrap();
    }
    run_loop(&mut mix, &mut cmds, &mut bus, 2).await;
    let id: SourceId = "odd".into();
    mix.take(Some(id.clone()), None).unwrap();
    run_loop(&mut mix, &mut cmds, &mut bus, 1).await;
    let programme = count_programme(&mix);
    let pad = mix.pool.slots().iter().find(|s| s.source() == Some(&id)).unwrap().pad().clone();
    let picture = count_pad(&pad);

    let proxy = mix.sources.iter().find(|s| s.input.id == id).unwrap().input.video_proxy.clone();
    let entry = proxy.static_pad("sink").unwrap().peer().expect("the proxy sink is linked");
    let bytes = gst::FormattedSegment::<gst::format::Bytes>::new();
    entry.push_event(gst::event::Segment::new(&bytes));
    let (p0, s0) = (programme.load(Ordering::Relaxed), picture.load(Ordering::Relaxed));
    let restarted = run_loop(&mut mix, &mut cmds, &mut bus, 8).await;
    let (p1, s1) = (programme.load(Ordering::Relaxed), picture.load(Ordering::Relaxed));
    run_loop(&mut mix, &mut cmds, &mut bus, 1).await;
    let s2 = picture.load(Ordering::Relaxed);
    let state = mix.sources.iter().find(|s| s.input.id == id).map(|s| s.input.observed_state());
    mix.shutdown();

    // Eight seconds at 30 fps is 240 frames. A stopped programme makes none;
    // the Windows runner, with the suite beside it, made 192, which is a
    // programme that ran slowly and never stopped. So the 200 is divided by
    // the slack a slow runner declares.
    let floor = (200.0 / crate::plugin::harness::timing_slack()) as u64;
    assert!(p1 > p0 + floor, "the programme stopped: {p0} then {p1} frames over eight seconds");
    assert!(restarted, "the source was not restarted");
    assert_eq!(state, Some(SourceState::Live), "the source did not come back");
    assert!(s2 > s1, "the source's picture did not come back: {s0}, {s1}, {s2}");
}
