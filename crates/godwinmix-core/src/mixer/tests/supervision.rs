//! The deadlines in `mixer::supervise`, against real pipelines.

use super::cable::Cable;
use super::*;

/// A mixer driven the way its own thread drives it: commands from its timers
/// and workers handled as they arrive, and a supervisor tick every `TICK`.
pub(super) struct Rig {
    pub mix: Mixer,
    rx: mpsc::Receiver<Command>,
    next_tick: Instant,
}

impl Rig {
    pub fn new(cfg: crate::config::Config) -> Self {
        let _ = gst::init();
        let (mut mix, _handle, rx, _bus) = Mixer::build(cfg).expect("mixer builds");
        mix.start().expect("the programme starts");
        Self { mix, rx, next_tick: Instant::now() }
    }

    pub fn add(&mut self, toml_src: &str) {
        let src: SourceConfig = toml::from_str(toml_src).expect("a source config");
        self.mix.add_source(&src, None).expect("the source is added");
    }

    pub async fn run(&mut self, how_long: Duration) {
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

    pub fn slot(&self, id: &str) -> Option<&SourceSlot> {
        self.mix.sources.iter().find(|s| s.input.id == id)
    }

    pub fn state(&self, id: &str) -> Option<SourceState> {
        self.slot(id).map(|s| s.input.observed_state())
    }

    /// Run until `id` reads `want`, for `within` at most. How long it took.
    pub async fn until(&mut self, id: &str, want: SourceState, within: Duration) -> Option<Duration> {
        let started = Instant::now();
        while started.elapsed() < within {
            if self.state(id) == Some(want) {
                return Some(started.elapsed());
            }
            self.run(Duration::from_millis(100)).await;
        }
        None
    }

    fn drawn(&self, id: &str) -> bool {
        self.mix.current_placements().iter().any(|p| p.source.as_str() == id)
    }
}

/// A pull into a relay that takes the connection and never answers is not
/// left on `connecting`: it is tried again, on the backoff, and again.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_that_never_connects_is_tried_again() {
    let dead = Cable::to(9, false);
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.stall.connect_timeout_secs = 2;
    let mut rig = Rig::new(cfg);
    rig.add(&format!(
        "id = \"far\"\nuri = \"rtmp://127.0.0.1:{}/live/far\"\nrtmp_client = \"rtmp2\"\n",
        dead.port
    ));
    rig.run(Duration::from_secs(12)).await;
    let tries = dead.accepted();
    let state = rig.state("far");
    rig.mix.shutdown();
    // The first try, then one about every 2.5 to 3.5 s with the backoff.
    assert!(tries >= 3, "only {tries} connect attempts in 12 s; the source reads {state:?}");
    assert_eq!(state, Some(SourceState::Connecting));
}

/// A source on air that stops holds its last frame, rather than dropping to
/// the slate the moment it is judged stalled, and gives way once the hold
/// is up.
#[tokio::test(flavor = "multi_thread")]
async fn a_stalled_source_holds_its_last_frame_then_gives_way() {
    let mut rig = Rig::new(programme_config(crate::config::Accel::Software));
    rig.add("id = \"held\"\nuri = \"test://ball\"\nstall_timeout_secs = 0.5\n");
    assert!(rig.until("held", SourceState::Live, Duration::from_secs(10)).await.is_some());
    rig.mix.take(Some("held".into()), None).unwrap();
    let input = rig.slot("held").unwrap().input.clone();
    let _ = input.pipeline.set_state(gst::State::Paused);
    assert!(rig.until("held", SourceState::Stalled, Duration::from_secs(10)).await.is_some());
    let held = rig.drawn("held");
    let long_ago = Instant::now().checked_sub(FREEZE_HOLD + Duration::from_secs(1));
    rig.mix.sources.iter_mut().for_each(|s| s.watch.live_at = long_ago);
    let after = rig.drawn("held");
    rig.mix.shutdown();
    assert!(held, "a stalled source was dropped from the programme at once");
    assert!(!after, "the frozen frame outlived its hold");
}

/// A restart that never comes back does not stop the source coming back: it
/// is left to its thread and the source is built again beside it.
#[tokio::test(flavor = "multi_thread")]
async fn a_hung_restart_is_abandoned_and_the_source_built_again() {
    let mut rig = Rig::new(programme_config(crate::config::Accel::Software));
    rig.add("id = \"stuck\"\nuri = \"test://ball\"\n");
    assert!(rig.until("stuck", SourceState::Live, Duration::from_secs(10)).await.is_some());
    let first = rig.slot("stuck").unwrap().generation;
    let input = rig.slot("stuck").unwrap().input.clone();
    // A restart claimed and never finished: what a teardown parked for good
    // looks like from the mixer.
    assert!(input.claim_restart());
    input.backdate_restart(super::supervise::RESTART_ABANDON);
    assert!(!rig.mix.arm_source_restart("stuck".into(), "a test"), "a restart was let in beside a hung one");
    rig.run(Duration::from_secs(1)).await;
    let back = rig.until("stuck", SourceState::Live, Duration::from_secs(20)).await;
    let now = rig.slot("stuck").map(|s| s.generation);
    rig.mix.shutdown();
    assert!(now.is_some_and(|g| g != first), "the source was not built again: {now:?}");
    assert!(back.is_some(), "the rebuilt source never went live");
}

/// An armed restart that is never spent gives way, so the next failure can
/// still schedule one.
#[tokio::test(flavor = "multi_thread")]
async fn a_spent_retry_leaves_the_source_free_to_be_retried() {
    let mut rig = Rig::new(programme_config(crate::config::Accel::Software));
    rig.add("id = \"again\"\nuri = \"test://ball\"\n");
    assert!(rig.until("again", SourceState::Live, Duration::from_secs(10)).await.is_some());
    let input = rig.slot("again").unwrap().input.clone();
    // The restart that the retry asks for finds one already running, which
    // left the claim set for good before 0.3.2.
    assert!(rig.mix.arm_source_restart("again".into(), "a test"));
    assert!(input.claim_restart());
    rig.run(Duration::from_secs(2)).await;
    input.restart_abandoned();
    let rearmed = rig.mix.arm_source_restart("again".into(), "a test");
    rig.mix.shutdown();
    assert!(rearmed, "a retry that could not run refused every retry after it");
}
