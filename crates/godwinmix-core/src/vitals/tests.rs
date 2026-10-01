//! A real programme, made black, frozen and silent on purpose, and the
//! `event/health` that comes out of each.

use std::time::{Duration, Instant};

use gstreamer as gst;
use serde_json::json;
use tokio::sync::broadcast;

use super::*;
use crate::config::{Config, SourceConfig};
use crate::mixer::{self, Command, Mixer};
use crate::state::{Envelope, SourceId};
use godwinmix_protocol::health::AlarmKind;

struct Show {
    handle: MixerHandle,
    events: broadcast::Receiver<Envelope>,
}

/// A 320x180 programme with three test sources and the multiview on, and
/// the vitals watching it with `limits` and the alarms on. Nothing is on
/// programme yet.
async fn show(limits: serde_json::Value) -> Show {
    show_with(true, limits).await
}

async fn show_with(alarms: bool, limits: serde_json::Value) -> Show {
    let _ = gst::init();
    let mut cfg: Config = toml::from_str("").unwrap();
    (cfg.canvas.width, cfg.canvas.height) = (320, 180);
    let (mut mix, handle, cmd_rx, mut bus_rx) = Mixer::build(cfg.clone()).expect("the mixer builds");
    mix.start().expect("the programme starts");
    for (id, pattern) in [("dark", "black"), ("still", "smpte75"), ("bars", "smpte")] {
        let src: SourceConfig = toml::from_str(&format!("id = \"{id}\"\nuri = \"test://{pattern}\"\n")).unwrap();
        mix.add_source(&src, None).unwrap();
    }
    let forward = handle.clone();
    tokio::spawn(async move {
        while let Some(ev) = bus_rx.recv().await {
            if forward.send(Command::Bus(ev)).is_err() {
                return;
            }
        }
    });
    let tracker = Tracker::new(cfg.snapshot.clone(), mix.multiview_handle(), handle.clone());
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));
    let events = handle.subscribe();
    let cfg = VitalsConfig { alarms, thresholds: serde_json::from_value(limits).unwrap() };
    tokio::spawn(run(handle.clone(), tracker, Shared::new(cfg)));
    Show { handle, events }
}

impl Show {
    async fn take(&self, id: &str) {
        let source = Some(SourceId::from(id));
        self.handle.request(move |ack| Command::Take { source, at_running_time_ms: None, ack: Some(ack) }).await.unwrap();
    }

    async fn mute(&self, id: &str) {
        self.handle.set_audio(SourceId::from(id), None, Some(true), None, Vec::new()).await.unwrap();
    }

    /// Wait for an `event/health` whose alarm kinds are exactly `want`.
    async fn until(&mut self, want: &[AlarmKind], limit: Duration) {
        let seen = self.kinds_within(limit, |kinds| kinds == want).await;
        assert!(seen.last().is_some_and(|k| k == want), "no event/health with {want:?} in {limit:?}: {seen:?}");
    }

    /// Every `event/health` for `limit`, or until `stop` says so, as its
    /// alarm kinds.
    async fn kinds_within(&mut self, limit: Duration, stop: impl Fn(&[AlarmKind]) -> bool) -> Vec<Vec<AlarmKind>> {
        let (deadline, mut seen) = (Instant::now() + limit, Vec::new());
        while Instant::now() < deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            let Ok(Ok(envelope)) = tokio::time::timeout(left, self.events.recv()).await else { continue };
            if let Event::Health { health } = envelope.event {
                seen.push(health.alarms.iter().map(|a| a.kind).collect::<Vec<_>>());
                if stop(seen.last().unwrap()) {
                    break;
                }
            }
        }
        seen
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_black_programme_raises_black_and_bars_clear_it() {
    let mut s = show(json!({"black_secs": 2, "freeze_secs": 0, "silence_secs": 0})).await;
    s.take("dark").await;
    s.until(&[AlarmKind::Black], Duration::from_secs(30)).await;
    s.take("bars").await;
    s.until(&[], Duration::from_secs(15)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_still_programme_freezes_and_moving_bars_clear_it() {
    let mut s = show(json!({"black_secs": 0, "freeze_secs": 3, "silence_secs": 0})).await;
    s.take("still").await;
    s.until(&[AlarmKind::Freeze], Duration::from_secs(30)).await;
    s.take("bars").await;
    s.until(&[], Duration::from_secs(15)).await;
}

/// The slate has no sound to fall quiet; a source with sound on programme
/// that goes quiet does, and its tone coming back clears it.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_source_with_sound_on_programme_can_be_silent() {
    let mut s = show(json!({"black_secs": 0, "freeze_secs": 0, "silence_secs": 2})).await;
    let seen = s.kinds_within(Duration::from_secs(6), |_| false).await;
    assert!(!seen.is_empty() && seen.iter().all(Vec::is_empty), "nothing on programme is not silence: {seen:?}");
    s.take("bars").await;
    s.mute("bars").await;
    s.until(&[AlarmKind::Silence], Duration::from_secs(30)).await;
    s.handle.set_audio(SourceId::from("bars"), None, Some(false), None, Vec::new()).await.unwrap();
    s.until(&[], Duration::from_secs(15)).await;
}

/// With the alarms off, a muted source on programme raises nothing.
#[tokio::test(flavor = "multi_thread")]
async fn with_the_alarms_off_a_quiet_programme_raises_nothing() {
    let mut s = show_with(false, json!({"black_secs": 1, "freeze_secs": 1, "silence_secs": 1})).await;
    s.take("dark").await;
    s.mute("dark").await;
    let seen = s.kinds_within(Duration::from_secs(8), |_| false).await;
    assert!(!seen.is_empty() && seen.iter().all(Vec::is_empty), "{seen:?}");
}

/// This process's CPU seconds, from `ps`.
fn cpu_secs() -> f64 {
    let out = std::process::Command::new("ps").args(["-o", "time=", "-p", &std::process::id().to_string()]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    text.trim().split(':').fold(0.0, |acc, part| acc * 60.0 + part.parse::<f64>().unwrap_or(0.0))
}

/// What keeping the picture alarms on costs a 720p show with nobody looking:
/// `cargo test -p godwinmix-core --release --lib vitals::tests::cost -- --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn cost_of_the_picture_alarms_with_nobody_looking() {
    for alarms in [false, true] {
        let _ = gst::init();
        let mut cfg: Config = toml::from_str("").unwrap();
        (cfg.canvas.width, cfg.canvas.height) = (1280, 720);
        let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).unwrap();
        mix.start().unwrap();
        for id in ["a", "b", "c"] {
            mix.add_source(&toml::from_str(&format!("id = \"{id}\"\nuri = \"test://smpte\"\n")).unwrap(), None).unwrap();
        }
        let tracker = Tracker::new(cfg.snapshot.clone(), mix.multiview_handle(), handle.clone());
        let thread = mixer::spawn(mix, cmd_rx, handle.clone());
        let vitals = tokio::spawn(run(handle.clone(), tracker, Shared::new(VitalsConfig { alarms, ..Default::default() })));
        tokio::time::sleep(Duration::from_secs(5)).await;
        let (c0, t0) = (cpu_secs(), Instant::now());
        tokio::time::sleep(Duration::from_secs(20)).await;
        let pct = 100.0 * (cpu_secs() - c0) / t0.elapsed().as_secs_f64();
        eprintln!("BENCH 720p show, three sources, alarms {alarms}: {pct:.1}% of one core");
        vitals.abort();
        let _ = handle.send(Command::Shutdown);
        drop(thread);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
