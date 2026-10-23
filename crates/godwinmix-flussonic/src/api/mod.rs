//! `/streamer/api/v3/streams` in Flussonic's JSON shape, filled from what
//! the shows report. Every field a monitoring page that reads the v3 streams API uses is here, with
//! Flussonic's meaning: rates in kbit/s, `lifetime` in milliseconds,
//! transcoder bitrates in bit/s.

pub mod auth;
pub mod echo;
pub mod edit;
pub mod sanitised;

use crate::model::Stream;
use serde::Serialize;
use serde_json::{json, Value};

/// One track as Flussonic's `media_info` lists it. `bitrate` is kbit/s.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MediaTrack {
    pub content: &'static str,
    pub track_id: String,
    pub codec: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_gop: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bframes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Inputs only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u16>,
}

/// What one stream is doing now, as the binary reads it off its show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Live {
    /// Frames arrive on the input.
    pub input_live: bool,
    /// The HLS output is live.
    pub output_live: bool,
    pub bitrate_kbps: u32,
    pub input_kbps: u32,
    /// Milliseconds since the output last went live.
    pub lifetime_ms: u64,
    pub tracks_out: Vec<MediaTrack>,
    pub tracks_in: Vec<MediaTrack>,
    /// Unix milliseconds of each time the input was closed as scrambled,
    /// newest last.
    pub scrambled_at: Vec<i64>,
    /// Continuity errors per second over the last reading.
    pub error_rate: f64,
    pub no_data_s: u64,
    pub restarts: u32,
    /// What is wrong, in GodwinMix's words, when something is.
    pub error: Option<String>,
}

fn status(s: &Stream, l: &Live) -> &'static str {
    match () {
        _ if s.disabled => "stopped",
        _ if l.input_live && l.output_live => "running",
        _ => "starting",
    }
}

/// The `stats` object.
pub fn stats(s: &Stream, l: &Live) -> Value {
    let running = status(s, l) == "running";
    let stop_events: Vec<Value> = l.scrambled_at.iter().map(|t| json!({"code": "scrambled", "at": t})).collect();
    let url = s.inputs.first().map(|i| i.url.clone()).unwrap_or_default();
    let mut v = json!({
        "status": status(s, l),
        "alive": running && l.tracks_out.iter().any(|t| t.content == "video"),
        "bitrate": if running { l.bitrate_kbps } else { 0 },
        "input_bitrate": l.input_kbps,
        "lifetime": if running { l.lifetime_ms } else { 0 },
        "media_info": {"tracks": if running { l.tracks_out.clone() } else { Vec::new() }},
        "inputs": [{
            "url": url,
            "active": l.input_live,
            "media_info": {"tracks": l.tracks_in},
            "stop_events": stop_events,
        }],
        "input": {"error_rate": l.error_rate, "num_sec_no_data": l.no_data_s},
        "transcoder": {"restarts": l.restarts},
    });
    if let Some(e) = &l.error {
        v["last_error"] = json!(e);
    }
    v
}

/// One stream's whole answer: its config and its `stats`.
pub fn stream(s: &Stream, l: &Live) -> Value {
    let mut m = echo::stream(s);
    m.insert("stats".into(), stats(s, l));
    Value::Object(m)
}

/// `GET /streamer/api/v3/streams`.
pub fn list(streams: &[(Stream, Live)]) -> Value {
    let all: Vec<Value> = streams.iter().map(|(s, l)| stream(s, l)).collect();
    json!({"estimated_count": all.len(), "streams": all})
}

#[cfg(test)]
mod tests;
