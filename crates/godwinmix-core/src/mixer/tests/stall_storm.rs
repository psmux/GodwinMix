use super::*;

/// A mixer driven the way its own thread drives it: commands from its timers
/// and workers handled as they arrive, and a supervisor tick every `TICK`.
struct Rig {
    mix: Mixer,
    rx: mpsc::Receiver<Command>,
    next_tick: Instant,
}

impl Rig {
    async fn run(&mut self, how_long: Duration) {
        let until = Instant::now() + how_long;
        while Instant::now() < until {
            while let Ok(cmd) = self.rx.try_recv() {
                let _ = self.mix.handle(cmd);
            }
            if Instant::now() >= self.next_tick {
                self.mix.tick();
                self.next_tick += TICK;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn state(&self) -> Option<SourceState> {
        self.mix.sources.iter().find(|s| s.input.id == "flaky").map(|s| s.input.observed_state())
    }

    /// Run until the source reads `want`, for a minute at most.
    async fn until(&mut self, want: SourceState) -> bool {
        for _ in 0..600 {
            if self.state() == Some(want) {
                return true;
            }
            self.run(Duration::from_millis(100)).await;
        }
        false
    }
}

async fn rig_with_one_source(restart_after_secs: u64) -> Rig {
    let _ = gst::init();
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.stall.restart_after_secs = restart_after_secs;
    let (mut mix, _handle, rx, _bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    let src: SourceConfig =
        toml::from_str("id = \"flaky\"\nuri = \"test://ball\"\nstall_timeout_secs = 0.5\n").unwrap();
    mix.add_source(&src, None).unwrap();
    let mut rig = Rig { mix, rx, next_tick: Instant::now() };
    assert!(rig.until(SourceState::Live).await, "the source never went live");
    rig
}

/// An end of stream is not a stall. A clip that loops restarts at each end,
/// and its next loop must start as soon as it always did, not after the
/// backoff a failing source earns.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_for_an_end_of_stream_is_not_a_strike() {
    let mut rig = rig_with_one_source(1).await;
    let id = SourceId::from("flaky");
    for _ in 0..3 {
        assert!(rig.mix.arm_source_restart(id.clone(), "it reached the end of its stream"));
        rig.run(Duration::from_millis(1500)).await;
    }
    let strikes = rig.mix.patience.get(&id).map(|p| p.strikes()).unwrap_or(0);
    let attempts = rig.mix.source_attempts.get(&id).copied();
    let state = rig.state();
    rig.mix.shutdown();
    assert_eq!(strikes, 0, "an end of stream was counted as a stall");
    assert_eq!(attempts, Some(0), "a looping clip's restart delay was not cleared by its next frame");
    assert_eq!(state, Some(SourceState::Live));
}

/// The shape of 2026-10-05: a source that stalls, is restarted, delivers for
/// a moment and stalls again. Each restart that does not hold must make the
/// next one wait longer, where before every one came after the same ten
/// seconds and the backoff was cleared by the first frame in between.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_that_keeps_stalling_waits_longer_each_time() {
    let mut rig = rig_with_one_source(1).await;
    let mut waits = Vec::new();
    for round in 0..3 {
        let input = rig.mix.sources.iter().find(|s| s.input.id == "flaky").unwrap().input.clone();
        // A live pipeline in PAUSED delivers nothing, which is a stall the
        // restart in place cures by taking it through NULL to PLAYING.
        let _ = input.pipeline.set_state(gst::State::Paused);
        let stopped = Instant::now();
        assert!(rig.until(SourceState::Stalled).await, "round {round}: never stalled");
        assert!(rig.until(SourceState::Live).await, "round {round}: never came back");
        waits.push(stopped.elapsed());
        // Back for a second and a half: far short of earning its backoff back.
        rig.run(Duration::from_millis(1500)).await;
    }
    let id = SourceId::from("flaky");
    let strikes = rig.mix.patience.get(&id).map(|p| p.strikes());
    let attempts = rig.mix.source_attempts.get(&id).copied();
    rig.mix.shutdown();

    assert_eq!(strikes, Some(3), "every stall restart is a strike until the source holds");
    assert_eq!(attempts, Some(3), "the restart delay was cleared by a moment of life");
    // One second, then two, then four, each after the half second it takes
    // to be judged stalled and before the restart's own delay. Without the
    // strikes all three were the same: 2.5, 2.4 and 2.5 s on this machine,
    // against 2.5, 3.8 and 6.8 s with them.
    assert!(
        waits[2] >= waits[0] + Duration::from_secs(3),
        "the third restart came as fast as the first: {waits:?}"
    );
}
