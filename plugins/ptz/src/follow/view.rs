//! What a loop has seen, as the JSON the track tools answer with. Boxes are
//! normalised, 0 to 1 from the top left of the picture.

use godwinmix_track::{Fallback, FollowState, Rect, TrackState};
use serde_json::{json, Value};

use super::Seen;

pub fn rect(r: &Rect) -> Value {
    json!({"x": round(r.x), "y": round(r.y), "w": round(r.w), "h": round(r.h)})
}

fn round(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

/// `idle`, `following`, `holding`, or `fallback-stay`, `fallback-wide`,
/// `fallback-preset`.
pub fn state(s: &FollowState) -> &'static str {
    match s {
        FollowState::Idle => "idle",
        FollowState::Following => "following",
        FollowState::Holding => "holding",
        FollowState::Fallback(Fallback::Stay) => "fallback-stay",
        FollowState::Fallback(Fallback::Wide) => "fallback-wide",
        FollowState::Fallback(Fallback::Preset(_)) => "fallback-preset",
    }
}

fn track_state(s: TrackState) -> String {
    format!("{s:?}").to_lowercase()
}

/// The whole picture of one loop.
pub fn seen(camera: &str, s: &Seen) -> Value {
    let tracks: Vec<Value> = s
        .out
        .tracks
        .iter()
        .map(|t| json!({"id": t.id.to_string(), "box": rect(&t.rect), "confidence": round(t.score), "state": track_state(t.state), "followed": Some(t.id) == s.out.target}))
        .collect();
    json!({
        "camera": camera,
        "mode": if s.ended { "stopped" } else if s.steering { "following" } else { "observing" },
        "state": state(&s.out.state),
        "framing": s.framing.map(|f| f.slug()),
        "target": s.out.target.map(|t| t.to_string()),
        "tracks": tracks,
        "shot": s.out.shot.as_ref().map(rect),
        "ptz": {"pan": round(s.out.ptz.pan), "tilt": round(s.out.ptz.tilt), "zoom": round(s.out.ptz.zoom)},
        "frames": s.frames,
        "fps": round(s.fps),
        "step_ms": round(s.step_ms),
        "picture": {"width": s.size.0, "height": s.size.1, "from": s.from},
        "age_ms": s.last_frame.map(|t| t.elapsed().as_millis() as u64),
        "error": s.error,
        "detector": {"running": s.detector.running.slug(), "wanted": s.detector.wanted.slug(), "stage": s.detector.stage, "why": s.detector.why},
    })
}

/// One sentence for a person: "Following person-3, waist shot."
pub fn sentence(camera: &str, s: &Seen) -> String {
    if let Some(e) = &s.error {
        return format!("{camera}: no pictures. {e}");
    }
    let people = s.out.tracks.len();
    let framing = s.framing.map(|f| f.slug()).unwrap_or("waist");
    match (&s.out.target, s.steering) {
        (Some(t), true) => format!("{camera}: following {t}, {framing} shot. {people} in view."),
        (None, true) => format!("{camera}: tracking is on, nobody to follow yet ({}). {people} in view.", state(&s.out.state)),
        _ => format!("{camera}: {people} in view, not steering."),
    }
}
