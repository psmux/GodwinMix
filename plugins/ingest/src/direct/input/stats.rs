//! `InputStats`: the contract's numbers for one input, read about once a
//! second by `show.stats` and the wall.

use serde_json::{json, Value};

/// Where an input is in its life.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum State {
    /// Opening the address, or waiting for the first frame.
    #[default]
    Connecting,
    /// Frames are arriving.
    Live,
    /// It failed or went quiet, and will try again; `error` says why.
    Retrying,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Connecting => "connecting",
            State::Live => "live",
            State::Retrying => "retrying",
        }
    }
}

/// One program of an MPEG-TS multiplex, as its PAT, PMT and SDT describe it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgramInfo {
    pub number: u16,
    pub name: String,
    pub provider: String,
    /// What each elementary stream is: `h264`, `aac`, `ac3`, `teletext`.
    pub streams: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputStats {
    pub state: State,
    pub error: Option<String>,
    /// The transport rate where there is one (the chosen program of a TS
    /// feed, stuffing left out), the media rate otherwise.
    pub kbps: u32,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub video_codec: String,
    pub audio_codec: String,
    pub audio_channels: u32,
    /// Continuity counter jumps on the chosen program's PIDs, one per jump.
    pub cc_errors: u64,
    /// Packets the transport says never arrived: RTP sequence gaps, SRT or
    /// RIST losses it could not recover, TS packets for a bare TS feed.
    pub packets_lost: u64,
    /// The gap between the last two keyframes.
    pub keyframe_ms: Option<u64>,
    /// How long since the last video frame (the last audio frame, for a
    /// feed with no video). `None` before the first.
    pub last_frame_ms: Option<u64>,
    pub program: Option<u16>,
    pub programs: Vec<ProgramInfo>,
}

impl InputStats {
    pub fn json(&self) -> Value {
        let mut v = json!({
            "state": self.state.as_str(),
            "kbps": self.kbps,
            "fps": self.fps,
            "width": self.width,
            "height": self.height,
            "video_codec": self.video_codec,
            "audio_codec": self.audio_codec,
            "audio_channels": self.audio_channels,
            "cc_errors": self.cc_errors,
            "packets_lost": self.packets_lost,
            "keyframe_ms": self.keyframe_ms,
            "last_frame_ms": self.last_frame_ms,
        });
        if let Some(e) = &self.error {
            v["error"] = json!(e);
        }
        if let Some(p) = self.program {
            v["program"] = json!(p);
        }
        if !self.programs.is_empty() {
            v["programs"] = self
                .programs
                .iter()
                .map(|p| json!({"number": p.number, "name": p.name, "provider": p.provider, "streams": p.streams}))
                .collect();
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_json_has_every_contract_field_and_the_optional_ones_only_when_set() {
        let s = InputStats { state: State::Live, kbps: 8000, ..InputStats::default() };
        let v = s.json();
        for key in ["kbps", "fps", "width", "height", "video_codec", "audio_codec", "audio_channels",
                    "cc_errors", "packets_lost", "keyframe_ms", "last_frame_ms", "state"] {
            assert!(v.get(key).is_some(), "{key} missing");
        }
        assert!(v.get("programs").is_none() && v.get("error").is_none());
        let s = InputStats {
            programs: vec![ProgramInfo { number: 2, name: "News".into(), ..ProgramInfo::default() }],
            ..s
        };
        assert_eq!(s.json()["programs"][0]["name"], "News");
    }
}
