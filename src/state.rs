//! Observable state shared between the mixer and the control plane.
//!
//! Everything here is plain data that serialises to JSON. The UI is driven
//! entirely by `MixerStatus` snapshots plus an `Event` stream, so a browser
//! that reconnects mid-broadcast can rebuild its whole view from one snapshot.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub type SourceId = String;
pub type OutputId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceState {
    /// Pipeline is up but no media has arrived yet.
    Connecting,
    /// Buffers arriving within the stall timeout.
    Live,
    /// Was live, then went quiet. Its program pad is held at alpha 0 so the
    /// slate shows through rather than a frozen frame.
    Stalled,
    /// The input pipeline errored. A retry is scheduled.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputState {
    Connecting,
    Live,
    Reconnecting,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceStatus {
    pub id: SourceId,
    pub name: String,
    pub uri: String,
    pub state: SourceState,
    pub has_video: bool,
    pub has_audio: bool,
    /// Index into the multiview grid, or None while the source has no cell.
    pub cell: Option<u32>,
    /// Milliseconds since the last video buffer, or None if none has arrived.
    pub video_idle_ms: Option<u64>,
    /// Same for audio. `None` here while `has_audio` is true means the source
    /// advertised an audio track that never produced a decoded sample.
    pub audio_idle_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputStatus {
    pub id: OutputId,
    pub uri_host: String,
    pub state: OutputState,
    pub reconnects: u32,
    /// Seconds of encoded data waiting in the pre-muxer queue. A number that
    /// climbs and stays high means the destination cannot keep up.
    pub queue_secs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiviewStatus {
    pub enabled: bool,
    pub width: i32,
    pub height: i32,
    pub cols: u32,
    pub rows: u32,
    /// Cell index to source id, in reading order. Cell 0 is the program return
    /// when it is enabled.
    pub cells: Vec<CellAssignment>,
    /// Frame rate of the mosaic, so the UI can size its own expectations.
    pub fps: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellAssignment {
    pub index: u32,
    /// None means this cell is the program return.
    pub source: Option<SourceId>,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// An ad break, either armed for a future cue or currently on air.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdStatus {
    pub uri: String,
    /// Source to return to when the ad ends. None returns to the slate.
    pub return_to: Option<SourceId>,
    /// False while it is prerolled and waiting for its cue.
    pub on_air: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerStatus {
    /// Source currently on program, or None while the slate is showing.
    pub program: Option<SourceId>,
    pub sources: Vec<SourceStatus>,
    pub outputs: Vec<OutputStatus>,
    pub multiview: MultiviewStatus,
    pub uptime_secs: u64,
    /// Program pipeline running time. Cues are scheduled against this, not
    /// against wall clock, so a client can place a break on a known frame.
    pub running_time_ms: u64,
    pub backend: BackendInfo,
    /// Present while an ad break is armed or running.
    pub ad: Option<AdStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendInfo {
    pub video_decoder: String,
    pub video_encoder: String,
    pub audio_decoder: String,
    pub audio_encoder: String,
    pub hardware_accelerated: bool,
}

/// Pushed to every connected UI as it happens.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A full snapshot. Sent on connect and after any structural change.
    Status(Box<MixerStatus>),
    /// The program source changed. Carries the running time the cut landed on
    /// so the UI can show how close a scheduled take was to its mark.
    Took { source: Option<SourceId>, at_running_time_ms: u64 },
    SourceStateChanged { source: SourceId, state: SourceState },
    OutputStateChanged { output: OutputId, state: OutputState, reconnects: u32 },
    /// An ad break started or ended.
    AdBreakChanged { ad: Option<AdStatus> },
    /// Peak level per channel, in dBFS, from the program bus. The mosaic
    /// carries no audio, so this is how an operator confirms that what is
    /// going out actually has sound on it.
    AudioLevel { peak_db: Vec<f64> },
    /// Something went wrong that the operator should see.
    Alert { severity: Severity, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
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

/// Strip credentials out of an RTMP URI before it goes anywhere near the UI.
/// Stream keys live in the path of most CDN ingest URLs and must not be shown.
pub fn safe_uri_label(uri: &str) -> String {
    match uri.split_once("://") {
        Some((scheme, rest)) => {
            let hostport = rest.split('/').next().unwrap_or(rest);
            let host = hostport.rsplit('@').next().unwrap_or(hostport);
            format!("{scheme}://{host}/…")
        }
        None => "…".to_string(),
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

    /// The browser parses these shapes by hand, so lock them here. Serde
    /// flattens an internally tagged newtype variant, which means a Status
    /// event arrives as the snapshot itself plus a `type` field, not nested
    /// under a key. Getting that wrong silently blanks the whole UI.
    #[test]
    fn event_json_shape_matches_what_the_ui_parses() {
        let status = MixerStatus {
            program: Some("cam1".into()),
            sources: vec![],
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

    #[test]
    fn stream_keys_never_reach_the_ui() {
        assert_eq!(
            safe_uri_label("rtmp://a.rtmp.youtube.com/live2/abcd-secret-key"),
            "rtmp://a.rtmp.youtube.com/…"
        );
        assert_eq!(
            safe_uri_label("rtmp://user:password@ingest.example.com/app/key"),
            "rtmp://ingest.example.com/…"
        );
        assert_eq!(safe_uri_label("garbage"), "…");
    }
}
