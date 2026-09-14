//! The event bus and the liveness tracking behind it.
//!
//! The data types that used to live here (`MixerStatus`, `Event`, every status
//! record) moved to `crate::api::types`, so that one module is the single
//! source of truth for the wire format and can carry `schemars` derives. They
//! are re-exported here unchanged, so `crate::state::MixerStatus` still
//! resolves and no caller had to be rewritten.
//!
//! What stays is the machinery: the broadcast bus that stamps a sequence
//! number on every event, and `SourceHealth`, which is read off a streaming
//! thread and must never block.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

pub use crate::api::types::*;

/// One event with the sequence number it was published under.
///
/// Every subscriber sees the same number for the same event, which is what
/// lets a client say "I have everything up to 4821" and lets the core answer
/// `event/resync {from_seq}` when it cannot.
#[derive(Debug, Clone)]
pub struct Envelope {
    pub seq: u64,
    pub event: Event,
}

/// The append only event stream.
///
/// A `tokio::broadcast` under a counter. The counter is bumped by the sender,
/// once per event, so the sequence is a property of the stream rather than of
/// any one reader. Sending never blocks and never fails when nobody is
/// listening, which matters because most of the call sites are on the mixer
/// thread and a slow UI must not be able to stall a take.
#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Envelope>,
    seq: Arc<AtomicU64>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx, seq: Arc::new(AtomicU64::new(0)) }
    }

    /// Publish one event. The returned count is how many receivers took it,
    /// kept so that call sites reading `let _ = bus.send(..)` still compile.
    pub fn send(&self, event: Event) -> Result<usize, broadcast::error::SendError<Envelope>> {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        self.tx.send(Envelope { seq, event })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Envelope> {
        self.tx.subscribe()
    }

    /// The sequence number of the last event published. A snapshot taken now
    /// is current as of this number.
    pub fn seq(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }
}

/// Liveness tracking for one source.
///
/// Updated from a pad probe on the streaming thread, read by the watchdog. An
/// atomic rather than a lock because the probe runs once per frame on the hot
/// path and must never block on the control plane.
#[derive(Debug)]
pub struct SourceHealth {
    origin: Instant,
    /// Milliseconds since `origin` at the last video buffer. `NEVER` if none.
    last_video_ms: AtomicU64,
    last_audio_ms: AtomicU64,
}

const NEVER: u64 = u64::MAX;

impl SourceHealth {
    pub fn new(origin: Instant) -> Arc<Self> {
        Arc::new(Self {
            origin,
            last_video_ms: AtomicU64::new(NEVER),
            last_audio_ms: AtomicU64::new(NEVER),
        })
    }

    fn now_ms(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }

    pub fn mark_video(&self) {
        self.last_video_ms.store(self.now_ms(), Ordering::Relaxed);
    }

    pub fn mark_audio(&self) {
        self.last_audio_ms.store(self.now_ms(), Ordering::Relaxed);
    }

    pub fn saw_video(&self) -> bool {
        self.last_video_ms.load(Ordering::Relaxed) != NEVER
    }

    pub fn saw_audio(&self) -> bool {
        self.last_audio_ms.load(Ordering::Relaxed) != NEVER
    }

    /// Milliseconds since the last video buffer, or None if none has arrived.
    pub fn video_idle_ms(&self) -> Option<u64> {
        let last = self.last_video_ms.load(Ordering::Relaxed);
        (last != NEVER).then(|| self.now_ms().saturating_sub(last))
    }

    /// Milliseconds since the last audio buffer, or None if none has arrived.
    pub fn audio_idle_ms(&self) -> Option<u64> {
        let last = self.last_audio_ms.load(Ordering::Relaxed);
        (last != NEVER).then(|| self.now_ms().saturating_sub(last))
    }

    /// True once video has been seen and has then been quiet for longer than
    /// the timeout. A source that has never produced anything is `Connecting`,
    /// not stalled, which is a different thing to show the operator.
    pub fn is_stalled(&self, timeout_secs: f64) -> bool {
        match self.video_idle_ms() {
            Some(idle) => idle as f64 / 1000.0 > timeout_secs,
            None => false,
        }
    }

    pub fn reset(&self) {
        self.last_video_ms.store(NEVER, Ordering::Relaxed);
        self.last_audio_ms.store(NEVER, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_starts_unknown_not_stalled() {
        let h = SourceHealth::new(Instant::now());
        assert!(!h.saw_video());
        assert_eq!(h.video_idle_ms(), None);
        // Never having produced is Connecting, not Stalled.
        assert!(!h.is_stalled(0.0));
    }

    #[test]
    fn health_detects_a_stall_only_after_media_was_seen() {
        let h = SourceHealth::new(Instant::now());
        h.mark_video();
        assert!(h.saw_video());
        assert!(!h.is_stalled(10.0));
        // A zero timeout means any elapsed time counts as stalled.
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(h.is_stalled(0.0));
        h.reset();
        assert!(!h.is_stalled(0.0));
    }

    /// Every subscriber must see the same number for the same event, and the
    /// numbers must not repeat. A client that trusts `seq` to spot a gap gets
    /// nothing out of a counter that is per reader.
    #[tokio::test]
    async fn every_subscriber_sees_the_same_sequence() {
        let bus = EventBus::new(16);
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        assert_eq!(bus.seq(), 0);
        bus.send(Event::Took { source: Some("cam1".into()), at_running_time_ms: 10 }).unwrap();
        bus.send(Event::Took { source: None, at_running_time_ms: 20 }).unwrap();
        assert_eq!(bus.seq(), 2);
        for rx in [&mut a, &mut b] {
            assert_eq!(rx.recv().await.unwrap().seq, 1);
            assert_eq!(rx.recv().await.unwrap().seq, 2);
        }
    }

    /// Nobody listening is the normal case for a headless mixer, and it must
    /// not cost the caller anything or stop the counter moving.
    #[test]
    fn sending_into_an_empty_room_still_advances_the_sequence() {
        let bus = EventBus::new(4);
        assert!(bus.send(Event::AudioLevel { peak_db: vec![-6.0] }).is_err());
        assert_eq!(bus.seq(), 1);
    }

    /// The browser parses these shapes by hand, so lock them here. Serde
    /// flattens an internally tagged newtype variant, which means a Status
    /// event arrives as the snapshot itself plus a `type` field, not nested
    /// under a key. Getting that wrong silently blanks the whole UI.
    #[test]
    fn event_json_shape_matches_what_the_ui_parses() {
        let status = MixerStatus {
            program: Some("cam1".into()),
            sources: vec![SourceStatus {
                id: "page".into(),
                name: "Live game".into(),
                uri: "web+https://example.com/live-game".into(),
                state: SourceState::Live,
                has_video: true,
                has_audio: true,
                cell: Some(1),
                video_idle_ms: Some(12),
                audio_idle_ms: Some(9),
                superimposed: true,
                audio: Some(SourceAudio { page: 0.8, media: vec![1.0, 0.0] }),
                gain: 0.5,
                muted: true,
                seekable: false,
                position_ms: None,
                duration_ms: None,
                extra: Extra::new(),
            }],
            outputs: vec![],
            multiview: MultiviewStatus {
                enabled: true,
                width: 960,
                height: 540,
                cols: 2,
                rows: 1,
                cells: vec![],
                fps: 8,
            },
            uptime_secs: 12,
            running_time_ms: 4200,
            ad: None,
            backend: BackendInfo {
                video_decoder: "vtdec_hw".into(),
                video_encoder: "vtenc_h264_hw".into(),
                audio_decoder: "avdec_aac".into(),
                audio_encoder: "fdkaacenc".into(),
                hardware_accelerated: true,
            },
        };

        let v: serde_json::Value =
            serde_json::to_value(Event::Status(Box::new(status))).unwrap();
        assert_eq!(v["type"], "status");
        // Flattened, not nested: `v["program"]`, never `v["status"]["program"]`.
        assert_eq!(v["program"], "cam1");
        assert!(v.get("status").is_none(), "snapshot must not be nested");
        assert_eq!(v["multiview"]["fps"], 8);
        assert_eq!(v["running_time_ms"], 4200);
        assert_eq!(v["backend"]["hardware_accelerated"], true);
        // The source list drives both the row badges and the multiview labels,
        // and the UI reads these keys by name off the parsed object.
        assert_eq!(v["sources"][0]["id"], "page");
        assert_eq!(v["sources"][0]["state"], "live");
        // Always written, never skipped when false: the UI branches on it
        // directly and `undefined` would read as "not superimposed" by luck
        // rather than by contract.
        assert_eq!(v["sources"][0]["superimposed"], true);
        // The balance travels with the source row, so the UI can draw the
        // faders from the same snapshot it draws the badges from.
        assert_eq!(v["sources"][0]["audio"]["page"], 0.8);
        assert_eq!(v["sources"][0]["audio"]["media"][1], 0.0);
        // The fader and the mute ride along too, and on every source rather
        // than only a superimposed one. Always written, never skipped: the UI
        // draws a fader for every row and needs a number to draw it at.
        assert_eq!(v["sources"][0]["gain"], 0.5);
        assert_eq!(v["sources"][0]["muted"], true);

        let v: serde_json::Value = serde_json::to_value(Event::Took {
            source: None,
            at_running_time_ms: 4200,
        })
        .unwrap();
        assert_eq!(v["type"], "took");
        assert!(v["source"].is_null(), "a cut to black reports a null source");
        assert_eq!(v["at_running_time_ms"], 4200);

        let v: serde_json::Value =
            serde_json::to_value(Event::AudioLevel { peak_db: vec![-6.0, -7.5] }).unwrap();
        assert_eq!(v["type"], "audio_level");
        assert_eq!(v["peak_db"][1], -7.5);

        // The per-source meter, which the UI matches to a mosaic tile by id.
        // A different `type` from the program meter on purpose: one drives the
        // program bar, the other drives a bar per picture.
        let v: serde_json::Value = serde_json::to_value(Event::SourceAudioLevel {
            source: "cam1".into(),
            peak_db: vec![-12.0, -11.4],
        })
        .unwrap();
        assert_eq!(v["type"], "source_audio_level");
        assert_eq!(v["source"], "cam1");
        assert_eq!(v["peak_db"][0], -12.0);
        assert_eq!(v["peak_db"][1], -11.4);

        // The scrubber's own event. Four a second for a file, never for a
        // camera, so the UI can drag against it. The duration is written even
        // when it is null: the UI reads the key to decide whether it can draw a
        // track at all, and a missing key and a null one have to mean the same
        // thing there.
        let v: serde_json::Value = serde_json::to_value(Event::SourcePosition {
            source: "clip1".into(),
            position_ms: 12345,
            duration_ms: Some(154_000),
        })
        .unwrap();
        assert_eq!(v["type"], "source_position");
        assert_eq!(v["source"], "clip1");
        assert_eq!(v["position_ms"], 12345);
        assert_eq!(v["duration_ms"], 154_000);

        let v: serde_json::Value = serde_json::to_value(Event::SourcePosition {
            source: "clip1".into(),
            position_ms: 0,
            duration_ms: None,
        })
        .unwrap();
        assert!(
            v["duration_ms"].is_null(),
            "a clip whose duration is not known yet reports a null, not a zero: {v}"
        );

        let v: serde_json::Value = serde_json::to_value(Event::OutputStateChanged {
            output: "primary".into(),
            state: OutputState::Reconnecting,
            reconnects: 3,
        })
        .unwrap();
        assert_eq!(v["type"], "output_state_changed");
        assert_eq!(v["state"], "reconnecting");

        let v: serde_json::Value = serde_json::to_value(Event::SourceStateChanged {
            source: "cam1".into(),
            state: SourceState::Stalled,
        })
        .unwrap();
        assert_eq!(v["type"], "source_state_changed");
        assert_eq!(v["state"], "stalled");
    }

    /// A snapshot written before `superimposed` existed has to keep parsing:
    /// a saved status file, or a peer on an older build. Without the serde
    /// default that is a hard failure rather than a source that simply
    /// reports itself as not superimposed.
    #[test]
    fn a_snapshot_from_before_superimpose_still_parses() {
        let older = serde_json::json!({
            "id": "cam1", "name": "Camera 1", "uri": "rtmp://host/live/cam1",
            "state": "live", "has_video": true, "has_audio": false,
            "cell": 0, "video_idle_ms": 12, "audio_idle_ms": null,
        });
        let s: SourceStatus = serde_json::from_value(older).unwrap();
        assert!(!s.superimposed);
        // Same again for the balance: a row from before it existed reports no
        // levels, which is exactly what a camera reports today.
        assert_eq!(s.audio, None);
        // The fader defaults to unity and the mute to off, which is where those
        // sources were before either control existed. A gain defaulting to 0.0
        // would bring a saved desk back with every source silent.
        assert_eq!(s.gain, 1.0);
        assert!(!s.muted);
        // Same for the scrubber. A row from before it existed describes a source
        // nothing could scrub, which is the truthful answer for a camera.
        assert!(!s.seekable);
        assert_eq!(s.position_ms, None);
        assert_eq!(s.duration_ms, None);
        // And nothing the older build wrote is mistaken for plugin data.
        assert!(s.extra.is_empty());
    }

    /// A camera carries no position at all, and a clip carries both numbers.
    /// The UI draws a scrubber when `seekable` is true, so a camera that
    /// reported a position would be given a control that can only fail.
    #[test]
    fn only_a_seekable_source_carries_a_position() {
        let camera = SourceStatus {
            id: "cam1".into(),
            name: "Camera 1".into(),
            uri: "rtmp://host/live/cam1".into(),
            state: SourceState::Live,
            has_video: true,
            has_audio: true,
            cell: Some(2),
            video_idle_ms: Some(20),
            audio_idle_ms: Some(20),
            superimposed: false,
            audio: None,
            gain: 1.0,
            muted: false,
            seekable: false,
            position_ms: None,
            duration_ms: None,
            extra: Extra::new(),
        };
        let v = serde_json::to_value(&camera).unwrap();
        // Written even when false: the UI branches on it, and `undefined`
        // reading as "no scrubber" by luck is not a contract.
        assert_eq!(v["seekable"], false);
        assert!(v.get("position_ms").is_none(), "a camera has no position");
        assert!(v.get("duration_ms").is_none(), "a camera has no duration");

        let clip = SourceStatus {
            seekable: true,
            position_ms: Some(12_345),
            duration_ms: Some(154_000),
            ..camera
        };
        let v = serde_json::to_value(&clip).unwrap();
        assert_eq!(v["seekable"], true);
        assert_eq!(v["position_ms"], 12_345);
        assert_eq!(v["duration_ms"], 154_000);
        // A clip whose duration the demuxer has not worked out yet still
        // reports where it has got to.
        let early = SourceStatus { duration_ms: None, ..clip };
        let v = serde_json::to_value(&early).unwrap();
        assert_eq!(v["position_ms"], 12_345);
        assert!(v.get("duration_ms").is_none());
    }

    /// The seek endpoint's answer, which is also what rides in the event.
    #[test]
    fn the_seek_answer_leaves_an_unknown_duration_out() {
        let landed = SourcePositionState { position_ms: 42_000, duration_ms: Some(154_000) };
        let v = serde_json::to_value(&landed).unwrap();
        assert_eq!(v["position_ms"], 42_000);
        assert_eq!(v["duration_ms"], 154_000);
        let back: SourcePositionState = serde_json::from_value(v).unwrap();
        assert_eq!(back, landed);

        let unknown = SourcePositionState { position_ms: 0, duration_ms: None };
        let v = serde_json::to_value(&unknown).unwrap();
        assert_eq!(v["position_ms"], 0);
        assert!(v.get("duration_ms").is_none());
    }

    /// The endpoint's answer. The fader and the mute are always there, and the
    /// balance only on a source that has one: a camera reporting `"page": null`
    /// would have the UI drawing balance faders that control nothing.
    #[test]
    fn the_audio_answer_carries_a_balance_only_when_there_is_one() {
        let camera = SourceAudioState { gain: 0.75, muted: true, page: None, media: None };
        let v = serde_json::to_value(&camera).unwrap();
        assert_eq!(v["gain"], 0.75);
        assert_eq!(v["muted"], true);
        assert!(v.get("page").is_none(), "a camera must not carry a page gain");
        assert!(v.get("media").is_none(), "a camera must not carry media gains");

        let page = SourceAudioState {
            gain: 1.0,
            muted: false,
            page: Some(0.8),
            media: Some(vec![1.0, 0.0]),
        };
        let v = serde_json::to_value(&page).unwrap();
        assert_eq!(v["page"], 0.8);
        assert_eq!(v["media"][1], 0.0);
        let back: SourceAudioState = serde_json::from_value(v).unwrap();
        assert_eq!(back, page);
    }

    /// A source with nothing to balance leaves the key out altogether rather
    /// than writing a null. The UI shows the faders when the key is there, so
    /// a camera that reported `"audio": null` would be indistinguishable from
    /// a superimposed source whose levels had not been read yet.
    #[test]
    fn only_a_superimposed_source_carries_a_balance() {
        let camera = SourceStatus {
            id: "cam1".into(),
            name: "Camera 1".into(),
            uri: "rtmp://host/live/cam1".into(),
            state: SourceState::Live,
            has_video: true,
            has_audio: true,
            cell: Some(2),
            video_idle_ms: Some(20),
            audio_idle_ms: Some(20),
            superimposed: false,
            audio: None,
            gain: 1.0,
            muted: false,
            seekable: false,
            position_ms: None,
            duration_ms: None,
            extra: Extra::new(),
        };
        let v = serde_json::to_value(&camera).unwrap();
        assert!(v.get("audio").is_none(), "a camera must not carry a balance");
        // It does carry a fader though. Every source has one of those.
        assert_eq!(v["gain"], 1.0);
        assert_eq!(v["muted"], false);

        let page = SourceStatus {
            audio: Some(SourceAudio { page: 0.25, media: vec![1.0] }),
            superimposed: true,
            ..camera
        };
        let v = serde_json::to_value(&page).unwrap();
        assert_eq!(v["audio"]["page"], 0.25);
        assert_eq!(v["audio"]["media"].as_array().unwrap().len(), 1);
        // Round trip, because the CLI and the MCP server parse this back.
        let back: SourceStatus = serde_json::from_value(v).unwrap();
        assert_eq!(back.audio, Some(SourceAudio { page: 0.25, media: vec![1.0] }));
    }
}
