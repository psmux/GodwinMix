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
/// the vitals watching it with `limits`. Nothing is on programme yet.
async fn show(limits: serde_json::Value) -> Show {
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
    let cfg = VitalsConfig { alarms: true, thresholds: serde_json::from_value(limits).unwrap() };
    tokio::spawn(run(handle.clone(), tracker, cfg));
    Show { handle, events }
}

impl Show {
    async fn take(&self, id: &str) {
        let source = Some(SourceId::from(id));
        self.handle.request(move |ack| Command::Take { source, at_running_time_ms: None, ack: Some(ack) }).await.unwrap();
    }

    /// Wait for an `event/health` whose alarm kinds are exactly `want`.
    async fn until(&mut self, want: &[AlarmKind], limit: Duration) {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            let Ok(Ok(envelope)) = tokio::time::timeout(left, self.events.recv()).await else { continue };
            if let Event::Health { health } = envelope.event {
                let kinds: Vec<AlarmKind> = health.alarms.iter().map(|a| a.kind).collect();
                if kinds == want {
                    return;
                }
            }
        }
        panic!("no event/health with {want:?} in {limit:?}");
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

#[tokio::test(flavor = "multi_thread")]
async fn a_programme_with_nothing_on_it_is_silent_until_a_tone_is_taken() {
    let mut s = show(json!({"black_secs": 0, "freeze_secs": 0, "silence_secs": 2})).await;
    s.until(&[AlarmKind::Silence], Duration::from_secs(30)).await;
    s.take("bars").await;
    s.until(&[], Duration::from_secs(15)).await;
}
